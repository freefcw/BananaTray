use crate::models::{QuotaDetailSpec, QuotaInfo, QuotaLabelSpec, QuotaType};
use crate::providers::{ProviderError, ProviderResult};
use serde::Deserialize;

/// 小时窗口。`unit: 3`，长度看 `number`（5 就是 5 小时会话）。
const UNIT_HOUR: i64 = 3;
/// 月窗口。`unit: 5`，旧套餐的 MCP 月额度；`number` 不表示「几个月」。
const UNIT_MONTH: i64 = 5;
/// 周窗口。`unit: 6`。服务端把同一条 7 天窗口报成 `number: 1` 或 `number: 7`，
/// 都不能按 `number` 乘成 N 周。
const UNIT_WEEK: i64 = 6;

const STANDARD_SESSION_HOURS: i64 = 5;
const MIN_EPOCH_MILLIS: f64 = 1_000_000_000_000.0;

#[derive(Debug, Clone, PartialEq)]
pub(super) struct QuotaSnapshot {
    pub(super) quotas: Vec<QuotaInfo>,
    pub(super) tier: Option<String>,
}

#[derive(Deserialize)]
struct Envelope {
    code: Option<i64>,
    success: Option<bool>,
    data: Option<QuotaData>,
}

#[derive(Deserialize)]
struct QuotaData {
    #[serde(default)]
    limits: Option<Vec<Limit>>,
    level: Option<String>,
}

#[derive(Deserialize)]
struct Limit {
    /// 窗口种类（`CREDIT_LIMIT` / `TOKENS_LIMIT` / `TIME_LIMIT`）。
    /// 重置周期只看 `unit` / `number`，新种类不能把已知窗口丢掉，所以这里不参与分类。
    #[serde(rename = "type", default)]
    #[allow(dead_code)]
    kind: String,
    #[serde(default)]
    unit: i64,
    #[serde(default)]
    number: i64,
    usage: Option<f64>,
    #[serde(rename = "currentValue")]
    current_value: Option<f64>,
    remaining: Option<f64>,
    percentage: Option<f64>,
    #[serde(rename = "nextResetTime")]
    next_reset_time: Option<f64>,
}

enum Measurement {
    UsedPercent(f64),
    RemainingFraction(f64),
}

struct WindowDraft {
    sort_key: (u8, i64),
    base_key: String,
    label: QuotaLabelSpec,
    quota_type: QuotaType,
    measurement: Measurement,
    reset: Option<QuotaDetailSpec>,
}

pub(super) fn parse_quota_response(body: &str) -> ProviderResult<QuotaSnapshot> {
    let envelope: Envelope = serde_json::from_str(body)
        .map_err(|_| ProviderError::parse_failed("ZCode quota response"))?;

    if is_auth_code(envelope.code) {
        return Err(ProviderError::auth_required(Some(
            crate::models::FailureAdvice::ApiHttpError {
                status: envelope.code.unwrap_or(401).to_string(),
            },
        )));
    }

    if envelope.success == Some(false) || is_rejected_code(envelope.code) {
        return Err(
            match envelope.code.and_then(|code| i32::try_from(code).ok()) {
                Some(code) => ProviderError::fetch_failed_with_advice(
                    crate::models::FailureAdvice::ApiError { code },
                ),
                None => ProviderError::fetch_failed("zcode quota rejected"),
            },
        );
    }

    let Some(data) = envelope.data else {
        return Err(ProviderError::no_data());
    };
    let Some(limits) = data.limits else {
        return Err(ProviderError::no_data());
    };
    if limits.is_empty() {
        return Err(ProviderError::no_data());
    }

    let mut drafts = Vec::new();
    for limit in &limits {
        if let Some(draft) = window_draft(limit) {
            drafts.push(draft);
        }
    }
    if drafts.is_empty() {
        return Err(ProviderError::parse_failed("ZCode quota windows"));
    }

    drafts.sort_by_key(|draft| draft.sort_key);
    Ok(QuotaSnapshot {
        quotas: assign_stable_keys(drafts),
        tier: normalize_tier(data.level),
    })
}

fn is_auth_code(code: Option<i64>) -> bool {
    matches!(code, Some(401 | 403))
}

fn is_rejected_code(code: Option<i64>) -> bool {
    matches!(code, Some(code) if code != 0 && code != 200)
}

fn window_draft(limit: &Limit) -> Option<WindowDraft> {
    let (sort_rank, base_key, label, quota_type) = classify(limit.unit, limit.number)?;
    let measurement = measurement(limit)?;
    Some(WindowDraft {
        sort_key: (sort_rank, limit.number),
        base_key,
        label,
        quota_type,
        measurement,
        reset: reset_detail(limit.next_reset_time),
    })
}

/// 用 `unit` 区分重置周期，不用数组下标。
///
/// 小时窗口的长度是 `number`；周 / 月窗口的长度由 `unit` 决定。
fn classify(unit: i64, number: i64) -> Option<(u8, String, QuotaLabelSpec, QuotaType)> {
    match unit {
        UNIT_HOUR if number == STANDARD_SESSION_HOURS => Some((
            0,
            "session".to_string(),
            QuotaLabelSpec::Session,
            QuotaType::Session,
        )),
        UNIT_HOUR if number > 0 && number <= u32::MAX as i64 => Some((
            1,
            format!("rolling-{number}h"),
            QuotaLabelSpec::RollingHours {
                hours: number as u32,
            },
            QuotaType::Session,
        )),
        UNIT_WEEK if matches!(number, 1 | 7) => Some((
            2,
            "weekly".to_string(),
            QuotaLabelSpec::Weekly,
            QuotaType::Weekly,
        )),
        UNIT_MONTH => Some((
            3,
            "monthly".to_string(),
            QuotaLabelSpec::Monthly,
            QuotaType::Monthly,
        )),
        _ => None,
    }
}

fn measurement(limit: &Limit) -> Option<Measurement> {
    match limit.percentage {
        Some(percent) if percent.is_finite() && percent >= 0.0 => {
            Some(Measurement::UsedPercent(percent))
        }
        Some(percent) if percent.is_finite() => None,
        _ => remaining_fraction(limit).map(Measurement::RemainingFraction),
    }
}

fn remaining_fraction(limit: &Limit) -> Option<f64> {
    let usage = limit
        .usage
        .filter(|value| value.is_finite() && *value > 0.0)?;
    let remaining = limit
        .remaining
        .filter(|value| value.is_finite())
        .or_else(|| {
            limit
                .current_value
                .filter(|value| value.is_finite())
                .map(|used| usage - used)
        })?;
    Some(remaining / usage)
}

/// `nextResetTime` 是 epoch 毫秒。0 表示窗口还没开始，不编造重置时间。
fn reset_detail(next_reset_time: Option<f64>) -> Option<QuotaDetailSpec> {
    let millis = next_reset_time.filter(|value| value.is_finite() && *value > 0.0)?;
    if millis < MIN_EPOCH_MILLIS {
        return None;
    }
    let secs = (millis / 1000.0).floor() as i64;
    if secs <= 0 {
        return None;
    }
    Some(QuotaDetailSpec::ResetAt { epoch_secs: secs })
}

fn assign_stable_keys(drafts: Vec<WindowDraft>) -> Vec<QuotaInfo> {
    let mut seen = std::collections::HashMap::<String, u32>::new();
    drafts
        .into_iter()
        .map(|draft| {
            let count = seen.entry(draft.base_key.clone()).or_insert(0);
            *count += 1;
            let stable_key = if *count == 1 {
                draft.base_key
            } else {
                format!("{}-{}", draft.base_key, *count)
            };
            match draft.measurement {
                Measurement::UsedPercent(percent) => QuotaInfo::with_key_from_used_percent(
                    stable_key,
                    draft.label,
                    percent,
                    draft.quota_type,
                    draft.reset,
                ),
                Measurement::RemainingFraction(fraction) => {
                    QuotaInfo::with_key_from_remaining_fraction(
                        stable_key,
                        draft.label,
                        fraction,
                        draft.quota_type,
                        draft.reset,
                    )
                }
            }
        })
        .collect()
}

fn normalize_tier(level: Option<String>) -> Option<String> {
    let level = level
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())?;
    let mut chars = level.chars();
    let first = chars.next()?;
    let mut tier = first.to_uppercase().collect::<String>();
    tier.push_str(chars.as_str());
    Some(tier)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credit_plan_keeps_five_hour_and_weekly_resets_apart() {
        let body = r#"{
            "code": 200,
            "msg": "Operation successful",
            "success": true,
            "data": {
                "level": "lite",
                "limits": [
                    {
                        "type": "CREDIT_LIMIT",
                        "unit": 6,
                        "number": 1,
                        "usage": 10000,
                        "currentValue": 71,
                        "remaining": 9929,
                        "percentage": 1,
                        "nextResetTime": 1786660486998
                    },
                    {
                        "type": "CREDIT_LIMIT",
                        "unit": 3,
                        "number": 5,
                        "usage": 2000,
                        "currentValue": 71,
                        "remaining": 1929,
                        "percentage": 3,
                        "nextResetTime": 1786073946574
                    }
                ]
            }
        }"#;

        let snapshot = parse_quota_response(body).unwrap();
        assert_eq!(snapshot.tier.as_deref(), Some("Lite"));
        assert_eq!(snapshot.quotas.len(), 2);
        assert_eq!(snapshot.quotas[0].label_spec, QuotaLabelSpec::Session);
        assert_eq!(snapshot.quotas[0].stable_key, "session");
        assert_eq!(snapshot.quotas[0].quota_type, QuotaType::Session);
        assert_eq!(snapshot.quotas[0].used, 3.0);
        assert_eq!(
            snapshot.quotas[0].detail_spec,
            Some(QuotaDetailSpec::ResetAt {
                epoch_secs: 1_786_073_946
            })
        );
        assert_eq!(snapshot.quotas[1].label_spec, QuotaLabelSpec::Weekly);
        assert_eq!(snapshot.quotas[1].stable_key, "weekly");
        assert_eq!(snapshot.quotas[1].used, 1.0);
        assert_eq!(
            snapshot.quotas[1].detail_spec,
            Some(QuotaDetailSpec::ResetAt {
                epoch_secs: 1_786_660_486
            })
        );
    }

    #[test]
    fn legacy_plan_keeps_session_weekly_and_monthly_mcp() {
        let body = r#"{
            "code": 200,
            "success": true,
            "data": {
                "limits": [
                    {
                        "type": "TIME_LIMIT",
                        "unit": 5,
                        "number": 1,
                        "usage": 4000,
                        "currentValue": 1828,
                        "remaining": 2172,
                        "percentage": 45
                    },
                    {
                        "type": "TOKENS_LIMIT",
                        "unit": 6,
                        "number": 7,
                        "percentage": 22,
                        "nextResetTime": 1770648402389
                    },
                    {
                        "type": "TOKENS_LIMIT",
                        "unit": 3,
                        "number": 5,
                        "usage": 800000000,
                        "currentValue": 127694464,
                        "percentage": 15,
                        "nextResetTime": 1770648402389
                    }
                ]
            }
        }"#;

        let snapshot = parse_quota_response(body).unwrap();
        assert!(snapshot.tier.is_none());
        assert_eq!(
            snapshot
                .quotas
                .iter()
                .map(|quota| quota.stable_key.as_str())
                .collect::<Vec<_>>(),
            vec!["session", "weekly", "monthly"]
        );
        assert_eq!(snapshot.quotas[0].used, 15.0);
        assert_eq!(snapshot.quotas[1].used, 22.0);
        assert_eq!(snapshot.quotas[1].quota_type, QuotaType::Weekly);
        assert_eq!(snapshot.quotas[2].label_spec, QuotaLabelSpec::Monthly);
        assert_eq!(snapshot.quotas[2].used, 45.0);
        assert!(snapshot.quotas[2].detail_spec.is_none());
    }

    #[test]
    fn non_five_hour_window_is_not_labeled_as_five_hours() {
        let body = r#"{
            "code": 200,
            "data": {
                "limits": [
                    {"type": "CREDIT_LIMIT", "unit": 3, "number": 10, "percentage": 40}
                ]
            }
        }"#;
        let snapshot = parse_quota_response(body).unwrap();
        assert_eq!(
            snapshot.quotas[0].label_spec,
            QuotaLabelSpec::RollingHours { hours: 10 }
        );
        assert_eq!(snapshot.quotas[0].stable_key, "rolling-10h");
    }

    #[test]
    fn server_percentage_wins_over_credit_arithmetic() {
        let body = r#"{
            "code": 200,
            "data": {
                "limits": [
                    {
                        "type": "CREDIT_LIMIT",
                        "unit": 3,
                        "number": 5,
                        "usage": 2000,
                        "currentValue": 71,
                        "remaining": 1929,
                        "percentage": 3
                    }
                ]
            }
        }"#;
        let snapshot = parse_quota_response(body).unwrap();
        assert_eq!(snapshot.quotas[0].used, 3.0);
    }

    #[test]
    fn derives_percent_from_remaining_when_percentage_is_absent() {
        let body = r#"{
            "code": 200,
            "data": {
                "limits": [
                    {
                        "type": "CREDIT_LIMIT",
                        "unit": 6,
                        "number": 1,
                        "usage": 2000,
                        "remaining": 1000
                    }
                ]
            }
        }"#;
        let snapshot = parse_quota_response(body).unwrap();
        assert_eq!(snapshot.quotas[0].used, 50.0);
    }

    #[test]
    fn zero_reset_time_stays_absent() {
        let body = r#"{
            "code": 200,
            "data": {
                "limits": [
                    {"type": "TOKENS_LIMIT", "unit": 3, "number": 5, "percentage": 0, "nextResetTime": 0}
                ]
            }
        }"#;
        let snapshot = parse_quota_response(body).unwrap();
        assert!(snapshot.quotas[0].detail_spec.is_none());
    }

    #[test]
    fn second_timestamp_is_not_treated_as_milliseconds() {
        let body = r#"{
            "code": 200,
            "data": {
                "limits": [
                    {"type": "TOKENS_LIMIT", "unit": 3, "number": 5, "percentage": 0, "nextResetTime": 1786073946}
                ]
            }
        }"#;
        let snapshot = parse_quota_response(body).unwrap();
        assert!(snapshot.quotas[0].detail_spec.is_none());
    }

    #[test]
    fn unknown_window_does_not_hide_known_resets() {
        let body = r#"{
            "code": 200,
            "data": {
                "limits": [
                    {"type": "FUTURE_LIMIT", "unit": 9, "number": 1, "percentage": 80},
                    {"type": "CREDIT_LIMIT", "unit": 3, "number": 5, "percentage": 4}
                ]
            }
        }"#;
        let snapshot = parse_quota_response(body).unwrap();
        assert_eq!(snapshot.quotas.len(), 1);
        assert_eq!(snapshot.quotas[0].stable_key, "session");
    }

    #[test]
    fn duplicate_weekly_windows_get_distinct_keys() {
        let body = r#"{
            "code": 200,
            "data": {
                "limits": [
                    {"type": "CREDIT_LIMIT", "unit": 6, "number": 1, "percentage": 1},
                    {"type": "CREDIT_LIMIT", "unit": 6, "number": 7, "percentage": 2}
                ]
            }
        }"#;
        let snapshot = parse_quota_response(body).unwrap();
        assert_eq!(snapshot.quotas[0].stable_key, "weekly");
        assert_eq!(snapshot.quotas[1].stable_key, "weekly-2");
    }

    #[test]
    fn invalid_week_number_is_not_parsed_as_weekly() {
        let err = parse_quota_response(
            r#"{"code":200,"data":{"limits":[{"unit":6,"number":2,"percentage":1}]}}"#,
        )
        .unwrap_err();
        assert!(matches!(err, ProviderError::ParseFailed { .. }));
    }

    #[test]
    fn empty_limits_are_no_data() {
        let err = parse_quota_response(r#"{"code":200,"data":{"limits":[]}}"#).unwrap_err();
        assert_eq!(err, ProviderError::no_data());
    }

    #[test]
    fn unrecognized_windows_are_parse_failures() {
        let err = parse_quota_response(
            r#"{"code":200,"data":{"limits":[{"type":"CREDIT_LIMIT","unit":9,"number":1,"percentage":1}]}}"#,
        )
        .unwrap_err();
        assert!(matches!(err, ProviderError::ParseFailed { .. }));
    }

    #[test]
    fn auth_envelope_is_not_parsed_as_quota() {
        let err = parse_quota_response(r#"{"code":401,"success":false}"#).unwrap_err();
        assert!(matches!(err, ProviderError::AuthRequired { .. }));
    }

    #[test]
    fn malformed_body_is_parse_failed() {
        let err = parse_quota_response("not-json").unwrap_err();
        assert!(matches!(err, ProviderError::ParseFailed { .. }));
    }
}
