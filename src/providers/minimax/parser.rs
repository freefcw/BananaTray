use crate::models::{FailureAdvice, QuotaDetailSpec, QuotaInfo, QuotaType};
use crate::providers::ProviderError;
use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Deserialize)]
struct MiniMaxRemainsResponse {
    base_resp: BaseResp,
    model_remains: Option<Vec<ModelRemain>>,
}

#[derive(Deserialize)]
struct BaseResp {
    status_code: i32,
    status_msg: Option<String>,
}

#[derive(Deserialize)]
struct ModelRemain {
    model_name: String,
    current_interval_total_count: i64,
    current_interval_usage_count: i64,
    #[allow(dead_code)]
    remains_time: Option<i64>,
    end_time: Option<i64>,
}

pub(super) fn parse_remains_response(response_str: &str) -> Result<Vec<QuotaInfo>> {
    let resp: MiniMaxRemainsResponse = serde_json::from_str(response_str).with_context(|| {
        format!(
            "Failed to parse MiniMax API response: {}",
            response_str.chars().take(200).collect::<String>()
        )
    })?;

    if resp.base_resp.status_code != 0 {
        let msg = resp
            .base_resp
            .status_msg
            .unwrap_or_else(|| "unknown error".to_string());
        return Err(
            ProviderError::fetch_failed_with_advice(FailureAdvice::ApiError { message: msg })
                .into(),
        );
    }

    let model_remains = resp.model_remains.unwrap_or_default();
    if model_remains.is_empty() {
        return Err(ProviderError::no_data().into());
    }

    let quotas = model_remains
        .into_iter()
        .map(|model| {
            let total = model.current_interval_total_count;
            let remaining = model.current_interval_usage_count.clamp(0, total);
            let used = total - remaining;
            let reset_at = model.end_time.map(|ms| QuotaDetailSpec::ResetAt {
                epoch_secs: ms / 1000,
            });
            let label = model.model_name;
            QuotaInfo::with_details(
                label.clone(),
                used as f64,
                total as f64,
                QuotaType::ModelSpecific(label),
                reset_at,
            )
        })
        .collect();

    Ok(quotas)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_remains_response_success() {
        let body = r#"{
            "base_resp": {"status_code": 0},
            "model_remains": [
                {"model_name": "abab6.5s-chat", "current_interval_total_count": 100, "current_interval_usage_count": 25, "end_time": 1767225600000}
            ]
        }"#;
        let quotas = parse_remains_response(body).unwrap();
        assert_eq!(quotas.len(), 1);
        assert_eq!(quotas[0].used, 75.0);
    }
}
