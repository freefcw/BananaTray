use crate::models::{QuotaDetailSpec, QuotaInfo, QuotaLabelSpec, QuotaType};
use crate::providers::ProviderError;
use crate::utils::time_utils;
use anyhow::Result;

pub(super) fn parse_usage_response(body: &str) -> Result<Vec<QuotaInfo>> {
    let json: serde_json::Value = serde_json::from_str(body)
        .map_err(|_| ProviderError::parse_failed("usage-summary response"))?;

    let mut quotas = Vec::new();

    let membership_type = json
        .get("membershipType")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    let is_unlimited = json
        .get("isUnlimited")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let reset_at = json
        .get("billingCycleEnd")
        .and_then(|v| v.as_str())
        .and_then(time_utils::parse_iso8601_to_epoch)
        .map(|epoch_secs| QuotaDetailSpec::ResetAt { epoch_secs });

    let is_free_tier = membership_type.eq_ignore_ascii_case("free");
    let tier_label = membership_type.to_uppercase();

    if is_unlimited {
        quotas.push(QuotaInfo::with_details(
            QuotaLabelSpec::MonthlyTier {
                tier: tier_label.clone(),
            },
            0.0,
            1.0,
            QuotaType::General,
            Some(QuotaDetailSpec::Unlimited),
        ));
        return Ok(quotas);
    }

    let individual_usage = json.get("individualUsage");
    let limit_type = json.get("limitType").and_then(|v| v.as_str()).unwrap_or("");

    quotas.extend(parse_plan_quotas(
        individual_usage,
        &tier_label,
        is_free_tier,
        reset_at.clone(),
    ));
    if let Some(on_demand_quota) = parse_credit_quota(
        individual_usage.and_then(|usage| usage.get("onDemand")),
        QuotaLabelSpec::OnDemand,
        reset_at.clone(),
    ) {
        quotas.push(on_demand_quota);
    }
    if limit_type == "team" {
        if let Some(team_quota) = parse_credit_quota(
            json.get("teamUsage")
                .and_then(|usage| usage.get("onDemand")),
            QuotaLabelSpec::Team,
            reset_at,
        ) {
            quotas.push(team_quota);
        }
    }

    if quotas.is_empty() {
        return Err(ProviderError::no_data().into());
    }

    Ok(quotas)
}

/// 解析 plan 配额：优先拆成 Auto（自由模型）与 API（三方模型）两池；
/// 若响应缺少百分比字段，再回退到单一 used/limit 月度档。
///
/// free 档没有 included API 额度池（该额度只属于 Pro/Pro+/Ultra），
/// 上游仍会返回恒为 0 的 `apiPercentUsed`，因此 free 只在该值为 0 时隐藏 API 池。
fn parse_plan_quotas(
    individual_usage: Option<&serde_json::Value>,
    tier: &str,
    is_free_tier: bool,
    reset_at: Option<QuotaDetailSpec>,
) -> Vec<QuotaInfo> {
    let plan = match individual_usage.and_then(|usage| usage.get("plan")) {
        Some(plan) => plan,
        None => return Vec::new(),
    };
    if !plan
        .get("enabled")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        return Vec::new();
    }

    let auto_percent = plan
        .get("autoPercentUsed")
        .and_then(serde_json::Value::as_f64);
    // free 档的 API 池恒为 0，展示它只是噪音；但一旦真出现非零用量
    // （free 账号开通 on-demand、或 Cursor 调整 free 政策），仍要展示，
    // 隐藏真实消耗比多一条 0% 更危险。
    let api_percent = plan
        .get("apiPercentUsed")
        .and_then(serde_json::Value::as_f64)
        .filter(|percent| !is_free_tier || *percent > 0.0);

    if auto_percent.is_some() || api_percent.is_some() {
        let mut quotas = Vec::with_capacity(2);
        if let Some(used_percent) = auto_percent {
            quotas.push(QuotaInfo::from_used_percent(
                QuotaLabelSpec::SubscriptionUsage {
                    plan: tier.to_string(),
                    pool: "auto".into(),
                },
                used_percent,
                QuotaType::General,
                reset_at.clone(),
            ));
        }
        if let Some(used_percent) = api_percent {
            quotas.push(QuotaInfo::from_used_percent(
                QuotaLabelSpec::SubscriptionUsage {
                    plan: tier.to_string(),
                    pool: "api".into(),
                },
                used_percent,
                QuotaType::General,
                reset_at,
            ));
        }
        return quotas;
    }

    parse_legacy_plan_quota(plan, tier, is_free_tier, reset_at)
        .into_iter()
        .collect()
}

fn parse_legacy_plan_quota(
    plan: &serde_json::Value,
    tier: &str,
    is_free_tier: bool,
    reset_at: Option<QuotaDetailSpec>,
) -> Option<QuotaInfo> {
    let used = plan
        .get("used")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.0);
    let declared_limit = plan
        .get("limit")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.0);
    // free 档的 `breakdown.total`（included + bonus）会随用量增长，不是固定上限，
    // 拿它当 limit 会算出漂移的百分比；宁可无数据也不展示错误数字。
    let breakdown_limit = if is_free_tier {
        0.0
    } else {
        plan.get("breakdown")
            .and_then(|breakdown| breakdown.get("total"))
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0)
    };
    let limit = if declared_limit > 0.0 {
        declared_limit
    } else {
        breakdown_limit
    };
    if limit <= 0.0 {
        return None;
    }

    let used = if declared_limit == 0.0 {
        plan.get("totalPercentUsed")
            .and_then(serde_json::Value::as_f64)
            .map(|percent| (percent * limit / 100.0).round())
            .unwrap_or(used)
    } else {
        used
    };
    Some(QuotaInfo::with_details(
        QuotaLabelSpec::MonthlyTier {
            tier: tier.to_string(),
        },
        used,
        limit,
        QuotaType::General,
        reset_at,
    ))
}

fn parse_credit_quota(
    usage: Option<&serde_json::Value>,
    label: QuotaLabelSpec,
    reset_at: Option<QuotaDetailSpec>,
) -> Option<QuotaInfo> {
    let usage = usage?;
    if !usage
        .get("enabled")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        return None;
    }

    let used = usage
        .get("used")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.0);
    let limit = usage
        .get("limit")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.0);
    (limit > 0.0).then(|| QuotaInfo::with_details(label, used, limit, QuotaType::Credit, reset_at))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_unlimited_plan() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let body = r#"{"membershipType":"pro","isUnlimited":true,"billingCycleEnd":"2026-05-01T00:00:00Z"}"#;
        let quotas = parse_usage_response(body).unwrap();
        assert_eq!(quotas.len(), 1);
        assert_eq!(quotas[0].detail_spec, Some(QuotaDetailSpec::Unlimited));
    }

    #[test]
    fn test_parse_auto_and_api_pools() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let body = r#"{
            "membershipType":"pro",
            "isUnlimited":false,
            "billingCycleEnd":"2026-05-01T00:00:00Z",
            "individualUsage":{
                "plan":{
                    "enabled":true,
                    "used":40,
                    "limit":100,
                    "autoPercentUsed":12.5,
                    "apiPercentUsed":80,
                    "totalPercentUsed":55
                }
            }
        }"#;
        let quotas = parse_usage_response(body).unwrap();
        assert_eq!(quotas.len(), 2);
        assert_eq!(
            quotas[0].label_spec,
            QuotaLabelSpec::SubscriptionUsage {
                plan: "PRO".into(),
                pool: "auto".into(),
            }
        );
        assert!((quotas[0].used - 12.5).abs() < f64::EPSILON);
        assert!((quotas[0].limit - 100.0).abs() < f64::EPSILON);
        assert_eq!(
            quotas[1].label_spec,
            QuotaLabelSpec::SubscriptionUsage {
                plan: "PRO".into(),
                pool: "api".into(),
            }
        );
        assert!((quotas[1].used - 80.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_team_and_ondemand_with_pools() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let body = r#"{
            "membershipType":"business",
            "isUnlimited":false,
            "billingCycleEnd":"2026-05-01T00:00:00Z",
            "limitType":"team",
            "individualUsage":{
                "plan":{
                    "enabled":true,
                    "used":40,
                    "limit":100,
                    "autoPercentUsed":0,
                    "apiPercentUsed":100,
                    "totalPercentUsed":100
                },
                "onDemand":{"enabled":true,"used":5,"limit":20}
            },
            "teamUsage":{"onDemand":{"enabled":true,"used":10,"limit":50}}
        }"#;
        let quotas = parse_usage_response(body).unwrap();
        assert_eq!(quotas.len(), 4);
        assert_eq!(
            quotas[0].label_spec,
            QuotaLabelSpec::SubscriptionUsage {
                plan: "BUSINESS".into(),
                pool: "auto".into(),
            }
        );
        assert_eq!(
            quotas[1].label_spec,
            QuotaLabelSpec::SubscriptionUsage {
                plan: "BUSINESS".into(),
                pool: "api".into(),
            }
        );
        assert_eq!(quotas[2].label_spec, QuotaLabelSpec::OnDemand);
        assert_eq!(quotas[3].label_spec, QuotaLabelSpec::Team);
    }

    #[test]
    fn test_parse_legacy_plan_without_percent_pools() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let body = r#"{
            "membershipType":"pro",
            "isUnlimited":false,
            "billingCycleEnd":"2026-05-01T00:00:00Z",
            "individualUsage":{
                "plan":{"enabled":true,"used":40,"limit":100}
            }
        }"#;
        let quotas = parse_usage_response(body).unwrap();
        assert_eq!(quotas.len(), 1);
        assert_eq!(
            quotas[0].label_spec,
            QuotaLabelSpec::MonthlyTier { tier: "PRO".into() }
        );
        assert!((quotas[0].used - 40.0).abs() < f64::EPSILON);
        assert!((quotas[0].limit - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_empty_response_returns_error() {
        let body = r#"{"membershipType":"free","isUnlimited":false}"#;
        assert!(parse_usage_response(body).is_err());
    }

    /// free 档真实响应：`used` / `limit` 恒为 0，额度信息只在 percent 字段里，
    /// `apiPercentUsed` 恒为 0（free 没有 API 额度池），因此只展示 Auto 池。
    #[test]
    fn test_parse_free_tier_skips_api_pool() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let body = r#"{
            "membershipType":"free",
            "isUnlimited":false,
            "billingCycleEnd":"2026-05-01T00:00:00Z",
            "individualUsage":{
                "plan":{
                    "enabled":true,
                    "used":0,
                    "limit":0,
                    "remaining":0,
                    "breakdown":{"included":0,"bonus":11,"total":11},
                    "autoPercentUsed":11,
                    "apiPercentUsed":0,
                    "totalPercentUsed":5.5
                },
                "onDemand":{"enabled":false,"used":0,"limit":null,"remaining":null}
            }
        }"#;
        let quotas = parse_usage_response(body).unwrap();
        assert_eq!(quotas.len(), 1);
        assert_eq!(
            quotas[0].label_spec,
            QuotaLabelSpec::SubscriptionUsage {
                plan: "FREE".into(),
                pool: "auto".into(),
            }
        );
        assert!((quotas[0].used - 11.0).abs() < f64::EPSILON);
        assert!((quotas[0].limit - 100.0).abs() < f64::EPSILON);
    }

    /// free 档一旦真有 API 用量就必须展示：隐藏真实消耗比多一条 0% 更危险。
    #[test]
    fn test_parse_free_tier_keeps_nonzero_api_pool() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let body = r#"{
            "membershipType":"free",
            "isUnlimited":false,
            "individualUsage":{
                "plan":{"enabled":true,"autoPercentUsed":11,"apiPercentUsed":7.5}
            }
        }"#;
        let quotas = parse_usage_response(body).unwrap();
        assert_eq!(quotas.len(), 2);
        assert_eq!(
            quotas[1].label_spec,
            QuotaLabelSpec::SubscriptionUsage {
                plan: "FREE".into(),
                pool: "api".into(),
            }
        );
        assert!((quotas[1].used - 7.5).abs() < f64::EPSILON);
    }

    /// 付费档的 `apiPercentUsed = 0` 仍要展示：0 是真实用量，不是"无此池"。
    #[test]
    fn test_parse_paid_tier_keeps_zero_api_pool() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let body = r#"{
            "membershipType":"pro",
            "isUnlimited":false,
            "individualUsage":{
                "plan":{"enabled":true,"autoPercentUsed":30,"apiPercentUsed":0}
            }
        }"#;
        let quotas = parse_usage_response(body).unwrap();
        assert_eq!(quotas.len(), 2);
        assert_eq!(
            quotas[1].label_spec,
            QuotaLabelSpec::SubscriptionUsage {
                plan: "PRO".into(),
                pool: "api".into(),
            }
        );
        assert!((quotas[1].used - 0.0).abs() < f64::EPSILON);
    }

    /// free 档缺少 percent 字段时不得用会随用量增长的 `breakdown.total` 当 limit。
    #[test]
    fn test_parse_free_tier_ignores_breakdown_total_as_limit() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let body = r#"{
            "membershipType":"free",
            "isUnlimited":false,
            "individualUsage":{
                "plan":{
                    "enabled":true,
                    "used":0,
                    "limit":0,
                    "breakdown":{"included":0,"bonus":11,"total":11}
                }
            }
        }"#;
        assert!(parse_usage_response(body).is_err());
    }

    /// 付费档仍保留 `breakdown.total` 回退，避免回归已有行为。
    #[test]
    fn test_parse_paid_tier_uses_breakdown_total_fallback() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let body = r#"{
            "membershipType":"pro",
            "isUnlimited":false,
            "individualUsage":{
                "plan":{
                    "enabled":true,
                    "used":0,
                    "limit":0,
                    "breakdown":{"included":20,"bonus":0,"total":20},
                    "totalPercentUsed":50
                }
            }
        }"#;
        let quotas = parse_usage_response(body).unwrap();
        assert_eq!(quotas.len(), 1);
        assert_eq!(
            quotas[0].label_spec,
            QuotaLabelSpec::MonthlyTier { tier: "PRO".into() }
        );
        assert!((quotas[0].used - 10.0).abs() < f64::EPSILON);
        assert!((quotas[0].limit - 20.0).abs() < f64::EPSILON);
    }
}
