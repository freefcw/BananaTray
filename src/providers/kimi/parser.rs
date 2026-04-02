use crate::models::{QuotaInfo, QuotaType};
use crate::providers::ProviderError;
use crate::utils::time_utils;
use anyhow::{Context, Result};
use rust_i18n::t;
use serde::Deserialize;

#[derive(Deserialize)]
struct KimiUsageResponse {
    usages: Option<Vec<KimiUsage>>,
}

#[derive(Deserialize)]
struct KimiUsage {
    scope: Option<String>,
    detail: Option<KimiUsageDetail>,
    limits: Option<Vec<KimiLimit>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct KimiUsageDetail {
    limit: Option<String>,
    used: Option<String>,
    #[allow(dead_code)]
    remaining: Option<String>,
    reset_time: Option<String>,
}

#[derive(Deserialize)]
struct KimiLimit {
    window: Option<KimiWindow>,
    detail: Option<KimiUsageDetail>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct KimiWindow {
    duration: Option<u64>,
    time_unit: Option<String>,
}

const KIMI_TIER_ANDANTE: u64 = 1024;
const KIMI_TIER_MODERATO: u64 = 2048;
const KIMI_TIER_ALLEGRETTO: u64 = 7168;

pub(super) fn parse_usage_response(response_str: &str) -> Result<Vec<QuotaInfo>> {
    let resp: KimiUsageResponse = serde_json::from_str(response_str).with_context(|| {
        format!(
            "Failed to parse Kimi API response: {}",
            response_str.chars().take(200).collect::<String>()
        )
    })?;

    let usages = resp.usages.unwrap_or_default();
    let coding_usage = usages
        .iter()
        .find(|u| u.scope.as_deref() == Some("FEATURE_CODING"))
        .context("No FEATURE_CODING usage data found.")?;

    let mut quotas = Vec::new();

    if let Some(detail) = &coding_usage.detail {
        let used = parse_num(&detail.used);
        let limit = parse_num(&detail.limit);
        let tier = detect_tier(limit);
        let label = match tier {
            Some(t_name) => format!("{} ({})", t!("quota.label.weekly"), t_name),
            None => t!("quota.label.weekly").to_string(),
        };
        let reset_at = detail
            .reset_time
            .as_deref()
            .and_then(time_utils::format_reset_countdown);

        quotas.push(QuotaInfo::with_details(
            label,
            used,
            limit,
            QuotaType::Weekly,
            reset_at,
        ));
    }

    if let Some(limits) = &coding_usage.limits {
        for lim in limits {
            let is_5h_window = lim
                .window
                .as_ref()
                .map(|w| {
                    w.duration == Some(300) && w.time_unit.as_deref() == Some("TIME_UNIT_MINUTE")
                })
                .unwrap_or(false);

            if is_5h_window {
                if let Some(detail) = &lim.detail {
                    let used = parse_num(&detail.used);
                    let limit = parse_num(&detail.limit);
                    let reset_at = detail
                        .reset_time
                        .as_deref()
                        .and_then(time_utils::format_reset_countdown);

                    quotas.push(QuotaInfo::with_details(
                        t!("quota.label.session").to_string(),
                        used,
                        limit,
                        QuotaType::Session,
                        reset_at,
                    ));
                }
            }
        }
    }

    if quotas.is_empty() {
        return Err(ProviderError::no_data().into());
    }

    Ok(quotas)
}

fn parse_num(val: &Option<String>) -> f64 {
    val.as_deref()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0)
}

fn detect_tier(weekly_limit: f64) -> Option<&'static str> {
    match weekly_limit as u64 {
        KIMI_TIER_ANDANTE => Some("Andante"),
        KIMI_TIER_MODERATO => Some("Moderato"),
        KIMI_TIER_ALLEGRETTO => Some("Allegretto"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_usage_response_weekly_and_session() {
        rust_i18n::set_locale("en");
        let body = r#"{
            "usages": [{
                "scope": "FEATURE_CODING",
                "detail": {"limit": "2048", "used": "256", "resetTime": "2026-05-01T00:00:00Z"},
                "limits": [{
                    "window": {"duration": 300, "timeUnit": "TIME_UNIT_MINUTE"},
                    "detail": {"limit": "100", "used": "30", "resetTime": "2026-05-01T05:00:00Z"}
                }]
            }]
        }"#;
        let quotas = parse_usage_response(body).unwrap();
        assert_eq!(quotas.len(), 2);
        assert_eq!(quotas[0].label, "Weekly (Moderato)");
        assert_eq!(quotas[1].label, "Session (5h)");
    }
}
