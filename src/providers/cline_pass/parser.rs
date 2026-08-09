use crate::models::{QuotaDetailSpec, QuotaInfo, QuotaLabelSpec, QuotaType};
use crate::providers::{ProviderError, ProviderResult};
use crate::utils::time_utils;
use serde::Deserialize;

#[derive(Deserialize)]
struct UsageResponse {
    success: bool,
    data: Option<UsageData>,
}

#[derive(Deserialize)]
struct UsageData {
    limits: Vec<UsageLimit>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsageLimit {
    #[serde(rename = "type")]
    limit_type: String,
    percent_used: Option<f64>,
    resets_at: Option<String>,
}

pub(super) fn parse_usage_response(body: &str) -> ProviderResult<Vec<QuotaInfo>> {
    let response: UsageResponse = serde_json::from_str(body)
        .map_err(|_| ProviderError::parse_failed("ClinePass usage response"))?;
    if !response.success {
        return Err(ProviderError::parse_failed("ClinePass usage response"));
    }

    let limits = response
        .data
        .map(|data| data.limits)
        .ok_or_else(ProviderError::no_data)?;
    let mut session = None;
    let mut weekly = None;
    let mut monthly = None;

    for limit in limits {
        let (label, quota_type, slot) = match limit.limit_type.as_str() {
            "five_hour" => (QuotaLabelSpec::Session, QuotaType::Session, &mut session),
            "weekly" => (QuotaLabelSpec::Weekly, QuotaType::Weekly, &mut weekly),
            "monthly" => (QuotaLabelSpec::Monthly, QuotaType::Monthly, &mut monthly),
            _ => continue,
        };
        let used = limit
            .percent_used
            .ok_or_else(|| ProviderError::parse_failed("ClinePass limit percentUsed"))?;
        let reset = limit
            .resets_at
            .as_deref()
            .and_then(time_utils::parse_iso8601_to_epoch)
            .map(|epoch_secs| QuotaDetailSpec::ResetAt { epoch_secs });
        *slot = Some(QuotaInfo::from_used_percent(label, used, quota_type, reset));
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
