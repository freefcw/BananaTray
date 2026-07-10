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

    if let Some(plan_quota) = parse_plan_quota(individual_usage, tier_label, reset_at.clone()) {
        quotas.push(plan_quota);
    }
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

fn parse_plan_quota(
    individual_usage: Option<&serde_json::Value>,
    tier: String,
    reset_at: Option<QuotaDetailSpec>,
) -> Option<QuotaInfo> {
    let plan = individual_usage?.get("plan")?;
    if !plan
        .get("enabled")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        return None;
    }

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
        QuotaLabelSpec::MonthlyTier { tier },
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
    fn test_parse_team_and_ondemand() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let body = r#"{
            "membershipType":"business",
            "isUnlimited":false,
            "billingCycleEnd":"2026-05-01T00:00:00Z",
            "limitType":"team",
            "individualUsage":{
                "plan":{"enabled":true,"used":40,"limit":100},
                "onDemand":{"enabled":true,"used":5,"limit":20}
            },
            "teamUsage":{"onDemand":{"enabled":true,"used":10,"limit":50}}
        }"#;
        let quotas = parse_usage_response(body).unwrap();
        assert_eq!(quotas.len(), 3);
    }

    #[test]
    fn test_parse_empty_response_returns_error() {
        let body = r#"{"membershipType":"free","isUnlimited":false}"#;
        assert!(parse_usage_response(body).is_err());
    }
}
