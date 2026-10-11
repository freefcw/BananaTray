//! Claude Usage API 配额解析。
//!
//! API probe 负责凭据与传输；本模块负责宽松解析可选 usage 块，避免上游
//! 字段漂移影响传输和 token 处理。

use super::{UsageQuotaData, UsageResponse};
use crate::models::{QuotaDetailSpec, QuotaInfo, QuotaLabelSpec, QuotaType};
use crate::utils::time_utils;

/// 将 Usage 响应解析为领域配额。注入 now_secs 以便测试稳定地过滤过期数据。
pub(super) fn parse_usage(usage: UsageResponse, now_secs: i64) -> Vec<QuotaInfo> {
    let mut quotas = Vec::new();

    push_percent_quota(
        &mut quotas,
        usage.five_hour,
        QuotaLabelSpec::Session,
        QuotaType::Session,
    );
    push_percent_quota(
        &mut quotas,
        usage.seven_day,
        QuotaLabelSpec::Weekly,
        QuotaType::Weekly,
    );
    push_percent_quota(
        &mut quotas,
        usage.seven_day_sonnet,
        QuotaLabelSpec::WeeklyModel {
            model: "Sonnet".to_string(),
        },
        QuotaType::ModelSpecific("Sonnet".to_string()),
    );
    push_percent_quota(
        &mut quotas,
        usage.seven_day_opus,
        QuotaLabelSpec::WeeklyModel {
            model: "Opus".to_string(),
        },
        QuotaType::ModelSpecific("Opus".to_string()),
    );

    if let Some(extra) = usage.extra_usage {
        if extra.is_enabled == Some(true) {
            if let (Some(used_credits), Some(monthly_limit)) =
                (extra.used_credits, extra.monthly_limit)
            {
                quotas.push(QuotaInfo::with_details(
                    QuotaLabelSpec::ExtraUsage,
                    used_credits / 100.0,
                    monthly_limit / 100.0,
                    QuotaType::Credit,
                    None,
                ));
            }
        }
    }

    if let Some(value) = usage.iguana_necktie.as_ref() {
        if let Some(quota) = parse_cloud_credits(value, now_secs) {
            quotas.push(quota);
        }
    }
    if let Some(value) = usage.cedar_ember.as_ref() {
        if let Some(quota) = parse_limit_reset_credits(value, now_secs) {
            quotas.push(quota);
        }
    }

    quotas
}

fn push_percent_quota(
    quotas: &mut Vec<QuotaInfo>,
    data: Option<UsageQuotaData>,
    label: QuotaLabelSpec,
    kind: QuotaType,
) {
    if let Some(d) = data {
        if let Some(utilization) = d.utilization {
            let reset_at = d.resets_at.as_ref().and_then(|s| {
                time_utils::parse_iso8601_to_epoch(s)
                    .map(|epoch_secs| QuotaDetailSpec::ResetAt { epoch_secs })
            });
            quotas.push(QuotaInfo::from_used_percent(
                label,
                utilization,
                kind,
                reset_at,
            ));
        }
    }
}

/// 解析 Claude 云端赠送额度（iguana_necktie）。
fn parse_cloud_credits(value: &serde_json::Value, now_secs: i64) -> Option<QuotaInfo> {
    let limit = value.get("limit_dollars")?.as_f64()?;
    if !limit.is_finite() || limit <= 0.0 {
        return None;
    }

    if value
        .get("locked_reason")
        .and_then(|v| v.as_str())
        .is_some_and(|reason| !reason.trim().is_empty())
    {
        return None;
    }

    let reported_remaining = optional_dollar_amount(value, "remaining_dollars", limit)?;
    let remaining = match reported_remaining {
        Some(remaining) => remaining,
        None => {
            let used = optional_dollar_amount(value, "used_dollars", limit)??;
            limit - used
        }
    };

    let expiry = match value.get("resets_at") {
        None | Some(serde_json::Value::Null) => None,
        Some(raw) => {
            let epoch = raw.as_str().and_then(time_utils::parse_iso8601_to_epoch)?;
            if epoch <= now_secs {
                return None;
            }
            Some(epoch)
        }
    };

    Some(QuotaInfo::with_details(
        QuotaLabelSpec::CloudCredits,
        limit - remaining,
        limit,
        QuotaType::Credit,
        expiry
            .and_then(|epoch| time_utils::days_until_epoch(epoch, now_secs))
            .map(|days| QuotaDetailSpec::ExpiresInDays { days }),
    ))
}

/// 读取可选美元字段。缺失/null 与字段形态异常有不同语义。
fn optional_dollar_amount(value: &serde_json::Value, key: &str, limit: f64) -> Option<Option<f64>> {
    match value.get(key) {
        None | Some(serde_json::Value::Null) => Some(None),
        Some(raw) => match raw.as_f64() {
            Some(amount) if amount.is_finite() && (0.0..=limit).contains(&amount) => {
                Some(Some(amount))
            }
            _ => None,
        },
    }
}

struct ResetGrant {
    resets_left: u64,
    paused: bool,
    starts_at: Option<i64>,
    ends_at: Option<i64>,
}

impl ResetGrant {
    fn is_available(&self, now_secs: i64) -> bool {
        !self.paused
            && self.resets_left > 0
            && self.starts_at.is_none_or(|start| start <= now_secs)
            && self.ends_at.is_none_or(|end| end > now_secs)
    }
}

fn parse_reset_grant(value: &serde_json::Value) -> Option<ResetGrant> {
    let resets_left = value.get("resets_left")?.as_u64()?;
    match value.get("resets_total") {
        None | Some(serde_json::Value::Null) => {}
        Some(raw) => match raw.as_u64() {
            Some(total) if total >= resets_left => {}
            _ => return None,
        },
    }
    let paused = value.get("paused")?.as_bool()?;
    Some(ResetGrant {
        resets_left,
        paused,
        starts_at: parse_grant_bound(value, "starts_at")?,
        ends_at: parse_grant_bound(value, "ends_at")?,
    })
}

fn parse_grant_bound(value: &serde_json::Value, key: &str) -> Option<Option<i64>> {
    match value.get(key) {
        None | Some(serde_json::Value::Null) => Some(None),
        Some(raw) => raw
            .as_str()
            .and_then(time_utils::parse_iso8601_to_epoch)
            .map(Some),
    }
}

/// 解析 Claude 的 Reset for free 库存（cedar_ember）。
fn parse_limit_reset_credits(value: &serde_json::Value, now_secs: i64) -> Option<QuotaInfo> {
    if value.get("eligible").and_then(|v| v.as_bool()) != Some(true) {
        return None;
    }
    let grants = value.get("grants")?.as_array()?;
    if grants.len() > 200 {
        return None;
    }

    let mut count: u64 = 0;
    let mut next_expiry: Option<i64> = None;
    for grant_value in grants {
        let Some(grant) = parse_reset_grant(grant_value) else {
            continue;
        };
        if !grant.is_available(now_secs) {
            continue;
        }
        if grant.resets_left > 50 - count {
            return None;
        }
        count += grant.resets_left;
        if let Some(end) = grant.ends_at {
            next_expiry = Some(next_expiry.map_or(end, |earliest| earliest.min(end)));
        }
    }
    if count == 0 {
        return None;
    }

    Some(QuotaInfo::balance_only(
        QuotaLabelSpec::LimitResetCredits,
        count as f64,
        None,
        QuotaType::Count,
        next_expiry
            .and_then(|epoch| time_utils::days_until_epoch(epoch, now_secs))
            .map(|days| QuotaDetailSpec::ExpiresInDays { days }),
    ))
}
