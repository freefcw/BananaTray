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
fn parse_plan_quotas(
    individual_usage: Option<&serde_json::Value>,
    tier: &str,
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
    let api_percent = plan
        .get("apiPercentUsed")
        .and_then(serde_json::Value::as_f64);

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

    parse_legacy_plan_quota(plan, tier, reset_at)
        .into_iter()
        .collect()
}

fn parse_legacy_plan_quota(
    plan: &serde_json::Value,
    tier: &str,
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
    let breakdown_limit = plan
        .get("breakdown")
        .and_then(|breakdown| breakdown.get("total"))
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.0);
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
}
