use crate::models::{QuotaDetailSpec, QuotaInfo, QuotaLabelSpec, QuotaType};
use crate::providers::{ProviderError, ProviderResult};
use crate::utils::time_utils;
use serde::Deserialize;

#[derive(Deserialize)]
struct UsageResponse {
    usage: UsageWindows,
}

#[derive(Deserialize)]
struct UsageWindows {
    rolling: Option<UsageWindow>,
    weekly: Option<UsageWindow>,
    monthly: Option<UsageWindow>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsageWindow {
    /// used percent（官方字段名 `percent`，来自 `usagePercent`）
    percent: f64,
    resets_at: Option<String>,
    #[allow(dead_code)]
    status: Option<String>,
}

pub(super) fn parse_usage_response(body: &str) -> ProviderResult<Vec<QuotaInfo>> {
    let response: UsageResponse = serde_json::from_str(body)
        .map_err(|_| ProviderError::parse_failed("OpenCode Go usage response"))?;

    let mut session = None;
    let mut weekly = None;
    let mut monthly = None;

    if let Some(window) = response.usage.rolling {
        session = Some(quota_from_window(
            QuotaLabelSpec::Session,
            QuotaType::Session,
            window,
        )?);
    }
    if let Some(window) = response.usage.weekly {
        weekly = Some(quota_from_window(
            QuotaLabelSpec::Weekly,
            QuotaType::Weekly,
            window,
        )?);
    }
    if let Some(window) = response.usage.monthly {
        monthly = Some(quota_from_window(
            QuotaLabelSpec::Monthly,
            QuotaType::Monthly,
            window,
        )?);
    }

    let quotas = [session, weekly, monthly]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    if quotas.is_empty() {
        Err(ProviderError::no_data())
    } else {
        Ok(quotas)
    }
}

fn quota_from_window(
    label: QuotaLabelSpec,
    quota_type: QuotaType,
    window: UsageWindow,
) -> ProviderResult<QuotaInfo> {
    if !window.percent.is_finite() || window.percent < 0.0 {
        return Err(ProviderError::parse_failed("OpenCode Go usage percent"));
    }
    let reset = window
        .resets_at
        .as_deref()
        .and_then(time_utils::parse_iso8601_to_epoch)
        .map(|epoch_secs| QuotaDetailSpec::ResetAt { epoch_secs });
    Ok(QuotaInfo::from_used_percent(
        label,
        window.percent,
        quota_type,
        reset,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rolling_weekly_monthly_windows() {
        let body = r#"{
            "usage": {
                "rolling": {"status":"ok","percent":19.5,"resetsAt":"2026-08-12T10:00:00.000Z"},
                "weekly": {"status":"ok","percent":29.7,"resetsAt":"2026-08-16T00:00:00.000Z"},
                "monthly": {"status":"rate-limited","percent":100.0,"resetsAt":"2026-09-01T00:00:00.000Z"}
            }
        }"#;
        let quotas = parse_usage_response(body).unwrap();
        assert_eq!(quotas.len(), 3);
        assert_eq!(quotas[0].label_spec, QuotaLabelSpec::Session);
        assert_eq!(quotas[0].used, 19.5);
        assert_eq!(quotas[0].limit, 100.0);
        assert_eq!(quotas[1].label_spec, QuotaLabelSpec::Weekly);
        assert_eq!(quotas[2].label_spec, QuotaLabelSpec::Monthly);
        assert!(matches!(
            quotas[0].detail_spec,
            Some(QuotaDetailSpec::ResetAt { .. })
        ));
    }

    #[test]
    fn allows_partial_windows() {
        let body = r#"{"usage":{"weekly":{"percent":12.0}}}"#;
        let quotas = parse_usage_response(body).unwrap();
        assert_eq!(quotas.len(), 1);
        assert_eq!(quotas[0].label_spec, QuotaLabelSpec::Weekly);
    }

    #[test]
    fn rejects_empty_usage_object() {
        let err = parse_usage_response(r#"{"usage":{}}"#).unwrap_err();
        assert_eq!(err, ProviderError::no_data());
    }
}
