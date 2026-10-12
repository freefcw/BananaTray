//! Codex 可兑换限流重置的解析。
//!
//! 重置不是 `credits` 余额，也不是会话/周窗口的用量。usage 响应通常只带
//! `available_count`；类型、获取时间和到期时间在
//! `/wham/rate-limit-reset-credits` 或 app-server 的 `rateLimitResetCredits.credits`。

use crate::models::{RateLimitReset, RateLimitResetSnapshot};
use crate::utils::time_utils;
use serde_json::Value;
use std::cmp::Ordering;

/// 从 usage / RPC / 详情响应里的重置对象取出快照。
///
/// 同时接受 snake_case 与 camelCase。无法识别的字段被忽略，不让整次配额解析失败。
pub(super) fn snapshot_from_value(value: &Value) -> RateLimitResetSnapshot {
    if !value.is_object() {
        return RateLimitResetSnapshot::default();
    }

    let available_count =
        field(value, &["available_count", "availableCount"]).and_then(parse_count);
    let credits = field(value, &["credits"]);
    let details_known = credits.is_some_and(Value::is_array);
    let mut resets = credits
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(reset_from_value)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    sort_resets(&mut resets);

    RateLimitResetSnapshot {
        available_count,
        resets,
        details_known,
    }
}

/// 详情接口和 usage 一样，HTTP 层返回的是带头的原始响应。
///
/// 解析失败返回 None，调用方保留 usage 里已有的数量摘要。
pub(super) fn parse_reset_details_response(raw: &str) -> Option<RateLimitResetSnapshot> {
    let body = http_body(raw);
    if body.is_empty() {
        return None;
    }
    let json: Value = serde_json::from_str(body).ok()?;
    let snapshot = snapshot_from_value(&json);
    if !snapshot.details_known && snapshot.available_count.is_none() {
        return None;
    }
    Some(snapshot)
}

/// 在 usage JSON 或 app-server `account/rateLimits/read` 结果里找重置对象。
pub(super) fn find_reset_credits(root: &Value) -> Option<&Value> {
    field(root, &["rate_limit_reset_credits", "rateLimitResetCredits"]).or_else(|| {
        field(root, &["rate_limit", "rateLimits"]).and_then(|rate_limit| {
            field(
                rate_limit,
                &["rate_limit_reset_credits", "rateLimitResetCredits"],
            )
        })
    })
}

fn reset_from_value(value: &Value) -> Option<RateLimitReset> {
    if !value.is_object() {
        return None;
    }
    if !status_is_available(field(value, &["status"])) {
        return None;
    }

    let reset_type = field(value, &["reset_type", "resetType"])
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .unwrap_or_default()
        .to_string();
    let title = field(value, &["title"])
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string);

    Some(RateLimitReset {
        reset_type,
        title,
        granted_at_secs: field(value, &["granted_at", "grantedAt"]).and_then(parse_timestamp),
        expires_at_secs: field(value, &["expires_at", "expiresAt"]).and_then(parse_timestamp),
    })
}

/// 缺状态时保留：详情数组里的条目有时只给时间和类型。
/// 明确不是 `available` 的条目（已兑换、已过期）不展示。
fn status_is_available(status: Option<&Value>) -> bool {
    match status.and_then(Value::as_str) {
        None => true,
        Some(status) => status.trim().eq_ignore_ascii_case("available"),
    }
}

fn sort_resets(resets: &mut [RateLimitReset]) {
    resets.sort_by(
        |left, right| match (left.expires_at_secs, right.expires_at_secs) {
            (Some(left_expires), Some(right_expires)) => left_expires
                .cmp(&right_expires)
                .then_with(|| match (left.granted_at_secs, right.granted_at_secs) {
                    (Some(left_granted), Some(right_granted)) => left_granted.cmp(&right_granted),
                    (Some(_), None) => Ordering::Less,
                    (None, Some(_)) => Ordering::Greater,
                    (None, None) => Ordering::Equal,
                }),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        },
    );
}

fn field<'a>(value: &'a Value, names: &[&str]) -> Option<&'a Value> {
    names.iter().find_map(|name| value.get(*name))
}

fn parse_count(value: &Value) -> Option<u32> {
    if let Some(count) = value.as_u64() {
        return u32::try_from(count).ok();
    }
    if let Some(count) = value.as_i64() {
        return u32::try_from(count.max(0)).ok();
    }
    if let Some(count) = value.as_f64() {
        if count.is_finite() && (0.0..u32::MAX as f64).contains(&count) {
            return Some(count as u32);
        }
    }
    value
        .as_str()
        .and_then(|text| text.trim().parse::<u32>().ok())
}

fn parse_timestamp(value: &Value) -> Option<i64> {
    if let Some(text) = value.as_str() {
        return time_utils::parse_iso8601_to_epoch(text.trim());
    }
    let raw = if let Some(seconds) = value.as_i64() {
        seconds
    } else if let Some(seconds) = value.as_u64() {
        i64::try_from(seconds).ok()?
    } else {
        let seconds = value.as_f64().filter(|seconds| seconds.is_finite())?;
        seconds as i64
    };
    Some(normalize_epoch(raw))
}

/// app-server 用秒。个别响应会给毫秒，超过 1e10 时折成秒。
fn normalize_epoch(raw: i64) -> i64 {
    if raw.abs() >= 10_000_000_000 {
        raw / 1000
    } else {
        raw
    }
}

fn http_body(raw: &str) -> &str {
    if let Some(index) = raw.find("\r\n\r\n") {
        raw[index + 4..].trim()
    } else if let Some(index) = raw.find("\n\n") {
        raw[index + 2..].trim()
    } else {
        raw.trim()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_summary_keeps_count_without_pretending_to_know_details() {
        let raw = serde_json::json!({
            "available_count": 2
        });
        let snapshot = snapshot_from_value(&raw);
        assert_eq!(snapshot.available_count, Some(2));
        assert!(snapshot.resets.is_empty());
        assert!(!snapshot.details_known);
        assert!(snapshot.needs_detail_fetch());
    }

    #[test]
    fn detail_rows_keep_type_grant_and_expiry_and_skip_redeemed() {
        let raw = serde_json::json!({
            "available_count": 2,
            "credits": [
                {
                    "id": "later",
                    "reset_type": "codex_rate_limits",
                    "status": "available",
                    "granted_at": "2026-06-20T00:00:00Z",
                    "expires_at": "2026-07-20T00:00:00Z",
                    "title": "Full reset (Weekly + 5 hr)"
                },
                {
                    "id": "sooner",
                    "reset_type": "codex_rate_limits",
                    "status": "available",
                    "granted_at": "2026-06-17T00:00:00Z",
                    "expires_at": "2026-07-17T00:00:00Z",
                    "title": "Full reset (Weekly + 5 hr)"
                },
                {
                    "id": "used",
                    "reset_type": "codex_rate_limits",
                    "status": "redeemed",
                    "granted_at": "2026-06-01T00:00:00Z",
                    "expires_at": "2026-07-01T00:00:00Z"
                }
            ]
        });

        let snapshot = snapshot_from_value(&raw);
        assert!(snapshot.details_known);
        assert_eq!(snapshot.resets.len(), 2);
        assert_eq!(
            snapshot.resets[0].expires_at_secs,
            time_utils::parse_iso8601_to_epoch("2026-07-17T00:00:00Z")
        );
        assert_eq!(
            snapshot.resets[0].granted_at_secs,
            time_utils::parse_iso8601_to_epoch("2026-06-17T00:00:00Z")
        );
        assert_eq!(snapshot.resets[0].reset_type, "codex_rate_limits");
        assert_eq!(
            snapshot.resets[0].title.as_deref(),
            Some("Full reset (Weekly + 5 hr)")
        );
        assert!(!snapshot.needs_detail_fetch());
    }

    #[test]
    fn rpc_shape_accepts_camel_case_and_unix_seconds() {
        let raw = serde_json::json!({
            "availableCount": 1,
            "credits": [{
                "resetType": "codex_rate_limits",
                "status": "available",
                "grantedAt": 1_735_689_600,
                "expiresAt": 1_781_654_400_000_i64
            }]
        });
        let snapshot = snapshot_from_value(&raw);
        assert_eq!(snapshot.resets.len(), 1);
        assert_eq!(snapshot.resets[0].granted_at_secs, Some(1_735_689_600));
        assert_eq!(snapshot.resets[0].expires_at_secs, Some(1_781_654_400));
        assert!(snapshot.resets[0].title.is_none());
    }

    #[test]
    fn null_expiry_stays_missing() {
        let raw = serde_json::json!({
            "availableCount": 1,
            "credits": [{
                "resetType": "codex_rate_limits",
                "status": "AVAILABLE",
                "grantedAt": "2026-06-17T00:00:00Z",
                "expiresAt": null
            }]
        });
        let snapshot = snapshot_from_value(&raw);
        assert_eq!(snapshot.resets.len(), 1);
        assert!(snapshot.resets[0].expires_at_secs.is_none());
    }

    #[test]
    fn finds_reset_object_beside_rate_limit() {
        let usage = serde_json::json!({
            "rate_limit": { "primary_window": {} },
            "rate_limit_reset_credits": { "available_count": 1 }
        });
        let found = find_reset_credits(&usage).unwrap();
        assert_eq!(found["available_count"], 1);

        let rpc = serde_json::json!({
            "rateLimits": {},
            "rateLimitResetCredits": { "availableCount": 3 }
        });
        assert_eq!(find_reset_credits(&rpc).unwrap()["availableCount"], 3);
    }

    #[test]
    fn unusable_detail_body_does_not_replace_a_count() {
        assert!(parse_reset_details_response("not json").is_none());
        assert!(parse_reset_details_response("{\"unexpected\":true}").is_none());
        let parsed = parse_reset_details_response(
            "HTTP/1.1 200 OK\r\n\r\n{\"available_count\":1,\"credits\":[]}",
        )
        .unwrap();
        assert_eq!(parsed.available_count, Some(1));
        assert!(parsed.details_known);
        assert!(parsed.resets.is_empty());
    }
}
