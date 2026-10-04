use crate::models::{
    ErrorKind, FailureReason, ProviderFailure, ProviderId, QuotaDetailSpec, QuotaInfo, QuotaType,
    RefreshData,
};
use crate::refresh::{RefreshReason, RefreshResult};

use super::redact::scrub_history_detail;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryStatus {
    Success,
    Unavailable,
    Failed,
}

impl HistoryStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Unavailable => "unavailable",
            Self::Failed => "failed",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "success" => Some(Self::Success),
            "unavailable" => Some(Self::Unavailable),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryValueKind {
    Metered,
    Balance,
    NonNumeric,
}

impl HistoryValueKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Metered => "metered",
            Self::Balance => "balance",
            Self::NonNumeric => "non_numeric",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryUnit {
    Percentage,
    Currency,
    Amount,
}

impl HistoryUnit {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Percentage => "percentage",
            Self::Currency => "currency",
            Self::Amount => "amount",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "percentage" => Some(Self::Percentage),
            "currency" => Some(Self::Currency),
            "amount" => Some(Self::Amount),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct QuotaHistoryPoint {
    pub quota_key: String,
    pub quota_type: String,
    pub label_spec_json: String,
    pub value_kind: HistoryValueKind,
    pub unit: Option<HistoryUnit>,
    pub used: Option<f64>,
    pub remaining: Option<f64>,
    pub limit_value: Option<f64>,
    pub reset_at_secs: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QuotaHistorySample {
    pub captured_at_ms: i64,
    pub provider_id: String,
    pub status: HistoryStatus,
    pub error_kind: Option<String>,
    pub failure_reason: Option<String>,
    pub failure_reason_payload: Option<String>,
    pub failure_advice_json: Option<String>,
    pub detail: Option<String>,
    pub refresh_reason: Option<String>,
    pub points: Vec<QuotaHistoryPoint>,
}

/// 跳过的刷新不是一次新的拉取，返回 `None`。
pub fn capture_sample(
    id: &ProviderId,
    result: &RefreshResult,
    reason: Option<RefreshReason>,
    captured_at_ms: i64,
) -> Option<QuotaHistorySample> {
    let refresh_reason = reason.map(refresh_reason_code);
    let provider_id = id.id_key();
    let sample = match result {
        RefreshResult::Success { data } => QuotaHistorySample {
            captured_at_ms,
            provider_id,
            status: HistoryStatus::Success,
            error_kind: None,
            failure_reason: None,
            failure_reason_payload: None,
            failure_advice_json: None,
            detail: None,
            refresh_reason,
            points: points_from_refresh(data),
        },
        RefreshResult::Unavailable { failure } => failure_sample(
            provider_id,
            captured_at_ms,
            HistoryStatus::Unavailable,
            failure,
            None,
            refresh_reason,
        ),
        RefreshResult::Failed {
            failure,
            error_kind,
        } => failure_sample(
            provider_id,
            captured_at_ms,
            HistoryStatus::Failed,
            failure,
            Some(*error_kind),
            refresh_reason,
        ),
        RefreshResult::SkippedCooldown
        | RefreshResult::SkippedInFlight
        | RefreshResult::SkippedDisabled
        | RefreshResult::SkippedStale => return None,
    };
    Some(sample)
}

fn failure_sample(
    provider_id: String,
    captured_at_ms: i64,
    status: HistoryStatus,
    failure: &ProviderFailure,
    error_kind: Option<ErrorKind>,
    refresh_reason: Option<String>,
) -> QuotaHistorySample {
    let (failure_reason, failure_reason_payload) = failure_reason_payload(&failure.reason);
    QuotaHistorySample {
        captured_at_ms,
        provider_id,
        status,
        error_kind: error_kind.map(error_kind_code),
        failure_reason: Some(failure_reason.to_string()),
        failure_reason_payload,
        failure_advice_json: failure
            .advice
            .as_ref()
            .and_then(|advice| serde_json::to_string(advice).ok()),
        detail: failure.raw_detail.as_deref().map(scrub_history_detail),
        refresh_reason,
        points: Vec::new(),
    }
}

fn points_from_refresh(data: &RefreshData) -> Vec<QuotaHistoryPoint> {
    data.quotas.iter().map(point_from_quota).collect()
}

fn point_from_quota(quota: &QuotaInfo) -> QuotaHistoryPoint {
    let label_spec_json =
        serde_json::to_string(&quota.label_spec).unwrap_or_else(|_| "{}".to_string());
    let mut point = QuotaHistoryPoint {
        quota_key: quota.stable_key.clone(),
        quota_type: quota.quota_type.stable_key(),
        label_spec_json,
        value_kind: HistoryValueKind::NonNumeric,
        unit: None,
        used: None,
        remaining: None,
        limit_value: None,
        reset_at_secs: reset_at_secs(quota),
    };
    if quota.is_balance_only() {
        if let Some(remaining) = quota.remaining_balance.filter(|value| value.is_finite()) {
            point.value_kind = HistoryValueKind::Balance;
            point.unit = Some(if quota.quota_type == QuotaType::Credit {
                HistoryUnit::Currency
            } else {
                HistoryUnit::Amount
            });
            point.remaining = Some(remaining);
            return point;
        }
    }
    if quota.used.is_finite() && quota.limit.is_finite() && quota.limit > 0.0 {
        point.value_kind = HistoryValueKind::Metered;
        point.used = Some(quota.used);
        point.limit_value = Some(quota.limit);
        let percentage_limit = (quota.limit - 100.0).abs() < 1e-9;
        point.unit = Some(
            if quota.quota_type != QuotaType::Credit
                && quota.quota_type != QuotaType::Points
                && percentage_limit
            {
                HistoryUnit::Percentage
            } else if quota.quota_type == QuotaType::Credit {
                HistoryUnit::Currency
            } else {
                HistoryUnit::Amount
            },
        );
        return point;
    }
    point
}

fn reset_at_secs(quota: &QuotaInfo) -> Option<i64> {
    match quota.detail_spec {
        Some(QuotaDetailSpec::ResetAt { epoch_secs }) => Some(epoch_secs),
        _ => None,
    }
}

fn refresh_reason_code(reason: RefreshReason) -> String {
    match reason {
        RefreshReason::Startup => "startup",
        RefreshReason::Periodic => "periodic",
        RefreshReason::Manual => "manual",
        RefreshReason::ProviderToggled => "provider_toggled",
    }
    .to_string()
}

fn error_kind_code(kind: ErrorKind) -> String {
    match kind {
        ErrorKind::ConfigMissing => "config_missing",
        ErrorKind::AuthRequired => "auth_required",
        ErrorKind::NetworkError => "network_error",
        ErrorKind::Unknown => "unknown",
    }
    .to_string()
}

pub(crate) fn failure_reason_payload(reason: &FailureReason) -> (&'static str, Option<String>) {
    match reason {
        FailureReason::CliNotFound { cli_name } => (
            "cli_not_found",
            Some(serde_json::json!({ "cli_name": cli_name }).to_string()),
        ),
        FailureReason::AuthRequired => ("auth_required", None),
        FailureReason::SessionExpired => ("session_expired", None),
        FailureReason::FolderTrustRequired => ("folder_trust_required", None),
        FailureReason::UpdateRequired { version } => (
            "update_required",
            Some(serde_json::json!({ "version": version }).to_string()),
        ),
        FailureReason::ConfigMissing { key } => (
            "config_missing",
            Some(serde_json::json!({ "key": key }).to_string()),
        ),
        FailureReason::Unavailable => ("unavailable", None),
        FailureReason::ParseFailed => ("parse_failed", None),
        FailureReason::Timeout => ("timeout", None),
        FailureReason::NoData => ("no_data", None),
        FailureReason::NetworkFailed => ("network_failed", None),
        FailureReason::FetchFailed => ("fetch_failed", None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{FailureAdvice, ProviderFailure, QuotaInfo, QuotaType};
    use crate::providers::ProviderError;

    fn failure(reason: FailureReason, advice: Option<FailureAdvice>) -> ProviderFailure {
        ProviderFailure {
            reason,
            advice,
            raw_detail: None,
        }
    }

    #[test]
    fn skipped_results_are_not_samples() {
        let id = ProviderId::from_id_key("codex");
        assert!(capture_sample(&id, &RefreshResult::SkippedCooldown, None, 1).is_none());
        assert!(capture_sample(&id, &RefreshResult::SkippedStale, None, 1).is_none());
    }

    #[test]
    fn success_without_quotas_keeps_an_empty_point_list() {
        let sample = capture_sample(
            &ProviderId::from_id_key("codex"),
            &RefreshResult::Success {
                data: RefreshData {
                    quotas: Vec::new(),
                    account_email: Some("a@b.co".to_string()),
                    account_tier: None,
                    source_label: None,
                },
            },
            Some(RefreshReason::Manual),
            10,
        )
        .unwrap();
        assert_eq!(sample.status, HistoryStatus::Success);
        assert!(sample.points.is_empty());
        assert_eq!(sample.refresh_reason.as_deref(), Some("manual"));
        assert_eq!(sample.provider_id, "codex");
    }

    #[test]
    fn percentage_count_and_balance_are_distinct() {
        let percent = point_from_quota(&QuotaInfo::from_used_percent(
            "session",
            30.0,
            QuotaType::General,
            None,
        ));
        assert_eq!(percent.unit, Some(HistoryUnit::Percentage));
        assert_eq!(percent.used, Some(30.0));

        let mut count = QuotaInfo::new("weekly", 200.0, 2048.0);
        count.quota_type = QuotaType::General;
        let count = point_from_quota(&count);
        assert_eq!(count.unit, Some(HistoryUnit::Amount));

        let balance = point_from_quota(&QuotaInfo::balance_only(
            "credit",
            12.5,
            None,
            QuotaType::Credit,
            None,
        ));
        assert_eq!(balance.value_kind, HistoryValueKind::Balance);
        assert_eq!(balance.unit, Some(HistoryUnit::Currency));
        assert_eq!(balance.remaining, Some(12.5));
        assert!(balance.used.is_none());
    }

    #[test]
    fn points_with_limit_100_stay_amount() {
        let mut quota = QuotaInfo::new("points", 20.0, 100.0);
        quota.quota_type = QuotaType::Points;
        let point = point_from_quota(&quota);
        assert_eq!(point.unit, Some(HistoryUnit::Amount));
    }

    #[test]
    fn every_provider_error_maps_onto_the_existing_failure_contract() {
        let cases = [
            ProviderError::cli_not_found("codex"),
            ProviderError::auth_required(Some(FailureAdvice::LoginCli {
                cli: "codex".into(),
            })),
            ProviderError::session_expired(None),
            ProviderError::FolderTrustRequired,
            ProviderError::update_required(Some("1.2")),
            ProviderError::update_required(None),
            ProviderError::config_missing("token"),
            ProviderError::config_mismatch("auth.mode"),
            ProviderError::unavailable("down"),
            ProviderError::parse_failed("bad json"),
            ProviderError::Timeout,
            ProviderError::no_data(),
            ProviderError::NetworkFailed {
                reason: "reset".into(),
            },
            ProviderError::fetch_failed("boom"),
        ];
        for error in cases {
            let failure = error.to_failure();
            let sample = capture_sample(
                &ProviderId::from_id_key("codex"),
                &RefreshResult::Failed {
                    failure: failure.clone(),
                    error_kind: error.error_kind(),
                },
                None,
                1,
            )
            .unwrap();
            let (code, payload) = failure_reason_payload(&failure.reason);
            assert_eq!(sample.failure_reason.as_deref(), Some(code));
            assert_eq!(sample.failure_reason_payload, payload);
            let expected_advice = failure
                .advice
                .as_ref()
                .and_then(|advice| serde_json::to_string(advice).ok());
            assert_eq!(sample.failure_advice_json, expected_advice);
            assert!(sample.points.is_empty());
        }
    }

    #[test]
    fn config_mismatch_is_stored_as_config_missing_payload() {
        let error = ProviderError::config_mismatch("auth.mode");
        let (code, payload) = failure_reason_payload(&error.to_failure().reason);
        assert_eq!(code, "config_missing");
        assert_eq!(payload.as_deref(), Some("{\"key\":\"auth.mode\"}"));
    }
}
