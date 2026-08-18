use crate::models::{QuotaDetailSpec, QuotaInfo, QuotaLabelSpec, QuotaType};
use crate::providers::{ProviderError, ProviderResult};
use crate::utils::time_utils;
use serde::Deserialize;

#[derive(Deserialize)]
struct BillingResponse {
    config: Option<BillingConfig>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BillingConfig {
    current_period: Option<UsagePeriod>,
    credit_usage_percent: Option<f64>,
    product_usage: Option<Vec<ProductUsage>>,
    billing_period_end: Option<String>,
}

#[derive(Deserialize)]
struct UsagePeriod {
    #[serde(rename = "type")]
    period_type: Option<String>,
    end: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProductUsage {
    product: Option<String>,
    usage_percent: Option<f64>,
}

pub(super) fn parse_usage_response(body: &str) -> ProviderResult<Vec<QuotaInfo>> {
    let response: BillingResponse = serde_json::from_str(body).map_err(|err| {
        log::warn!(
            target: "providers",
            "grok billing parse failed: {err}; bytes={}",
            body.len()
        );
        ProviderError::parse_failed("Grok billing response")
    })?;
    let Some(config) = response.config else {
        return Err(ProviderError::no_data());
    };

    let reset = reset_detail(
        config.current_period.as_ref(),
        config.billing_period_end.as_deref(),
    );
    let (period_label, period_type) = period_quota_type(
        config
            .current_period
            .as_ref()
            .and_then(|p| p.period_type.as_deref()),
    );

    let mut quotas = Vec::new();
    if let Some(percent) = config.credit_usage_percent {
        quotas.push(percent_quota(
            period_label,
            period_type,
            percent,
            reset.clone(),
        )?);
    }

    let products = config.product_usage.unwrap_or_default();
    if should_include_product_rows(quotas.first(), &products) {
        for product in products {
            let Some(name) = product.product.filter(|name| !name.is_empty()) else {
                continue;
            };
            let Some(percent) = product.usage_percent else {
                continue;
            };
            quotas.push(percent_quota(
                product_label(&name),
                QuotaType::General,
                percent,
                reset.clone(),
            )?);
        }
    }

    if quotas.is_empty() {
        Err(ProviderError::no_data())
    } else {
        Ok(quotas)
    }
}

fn period_quota_type(period_type: Option<&str>) -> (QuotaLabelSpec, QuotaType) {
    match period_type {
        Some("USAGE_PERIOD_TYPE_WEEKLY") => (QuotaLabelSpec::Weekly, QuotaType::Weekly),
        Some("USAGE_PERIOD_TYPE_MONTHLY") => (QuotaLabelSpec::Monthly, QuotaType::Monthly),
        Some("USAGE_PERIOD_TYPE_DAILY") => (QuotaLabelSpec::Daily, QuotaType::General),
        _ => (QuotaLabelSpec::Weekly, QuotaType::Weekly),
    }
}

fn should_include_product_rows(overall: Option<&QuotaInfo>, products: &[ProductUsage]) -> bool {
    // 总池已经覆盖「唯一产品且百分比相同」的情况，再展示只会重复。
    match (overall, products) {
        (Some(overall), [single]) => !single
            .usage_percent
            .is_some_and(|percent| (percent - overall.used).abs() < f64::EPSILON),
        (_, []) => false,
        _ => true,
    }
}

fn product_label(product: &str) -> QuotaLabelSpec {
    match product {
        "GrokBuild" => QuotaLabelSpec::Raw("Grok Build".into()),
        other => QuotaLabelSpec::Raw(other.to_string()),
    }
}

fn reset_detail(
    period: Option<&UsagePeriod>,
    billing_period_end: Option<&str>,
) -> Option<QuotaDetailSpec> {
    let iso = period
        .and_then(|period| period.end.as_deref())
        .or(billing_period_end)?;
    time_utils::parse_iso8601_to_epoch(iso)
        .map(|epoch_secs| QuotaDetailSpec::ResetAt { epoch_secs })
}

fn percent_quota(
    label: QuotaLabelSpec,
    quota_type: QuotaType,
    percent: f64,
    detail: Option<QuotaDetailSpec>,
) -> ProviderResult<QuotaInfo> {
    if !percent.is_finite() || percent < 0.0 {
        return Err(ProviderError::parse_failed("Grok usage percent"));
    }
    Ok(QuotaInfo::from_used_percent(
        label, percent, quota_type, detail,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const WEEKLY_CREDITS: &str = r#"{
        "config": {
            "currentPeriod": {
                "type": "USAGE_PERIOD_TYPE_WEEKLY",
                "start": "2026-08-16T10:55:32.222006+00:00",
                "end": "2026-08-23T10:55:32.222006+00:00"
            },
            "creditUsagePercent": 12.0,
            "onDemandCap": {"val": 0},
            "onDemandUsed": {"val": 0},
            "productUsage": [
                {"product": "GrokBuild", "usagePercent": 12.0}
            ],
            "isUnifiedBillingUser": true,
            "prepaidBalance": {"val": 0},
            "billingPeriodStart": "2026-08-16T10:55:32.222006+00:00",
            "billingPeriodEnd": "2026-08-23T10:55:32.222006+00:00"
        }
    }"#;

    #[test]
    fn parses_weekly_credits_pool_and_skips_duplicate_product() {
        let quotas = parse_usage_response(WEEKLY_CREDITS).unwrap();
        assert_eq!(quotas.len(), 1);
        assert_eq!(quotas[0].label_spec, QuotaLabelSpec::Weekly);
        assert_eq!(quotas[0].quota_type, QuotaType::Weekly);
        assert_eq!(quotas[0].used, 12.0);
        assert_eq!(quotas[0].limit, 100.0);
        assert!(matches!(
            quotas[0].detail_spec,
            Some(QuotaDetailSpec::ResetAt { epoch_secs }) if epoch_secs > 0
        ));
    }

    #[test]
    fn keeps_distinct_product_rows() {
        let body = r#"{
            "config": {
                "currentPeriod": {"type": "USAGE_PERIOD_TYPE_WEEKLY", "end": "2026-08-23T10:55:32Z"},
                "creditUsagePercent": 40.0,
                "productUsage": [
                    {"product": "GrokBuild", "usagePercent": 25.0},
                    {"product": "Chat", "usagePercent": 15.0}
                ]
            }
        }"#;
        let quotas = parse_usage_response(body).unwrap();
        assert_eq!(quotas.len(), 3);
        assert_eq!(quotas[0].label_spec, QuotaLabelSpec::Weekly);
        assert_eq!(
            quotas[1].label_spec,
            QuotaLabelSpec::Raw("Grok Build".into())
        );
        assert_eq!(quotas[1].used, 25.0);
        assert_eq!(quotas[2].label_spec, QuotaLabelSpec::Raw("Chat".into()));
        assert_eq!(quotas[2].used, 15.0);
    }

    #[test]
    fn uses_product_rows_when_overall_percent_missing() {
        let body = r#"{
            "config": {
                "productUsage": [{"product": "GrokBuild", "usagePercent": 8.0}]
            }
        }"#;
        let quotas = parse_usage_response(body).unwrap();
        assert_eq!(quotas.len(), 1);
        assert_eq!(
            quotas[0].label_spec,
            QuotaLabelSpec::Raw("Grok Build".into())
        );
        assert_eq!(quotas[0].used, 8.0);
    }

    #[test]
    fn rejects_empty_config() {
        assert_eq!(
            parse_usage_response(r#"{"config":{}}"#).unwrap_err(),
            ProviderError::no_data()
        );
    }

    #[test]
    fn rejects_invalid_json() {
        assert_eq!(
            parse_usage_response("not-json").unwrap_err(),
            ProviderError::parse_failed("Grok billing response")
        );
    }
}
