use crate::models::{FailureAdvice, QuotaDetailSpec, QuotaInfo, QuotaType};
use crate::providers::{ProviderError, ProviderResult};
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

pub(super) fn parse_remains_response(response_str: &str) -> ProviderResult<Vec<QuotaInfo>> {
    let resp: MiniMaxRemainsResponse = serde_json::from_str(response_str)
        .map_err(|_| ProviderError::parse_failed("MiniMax API response"))?;

    if resp.base_resp.status_code != 0 {
        let msg = resp
            .base_resp
            .status_msg
            .unwrap_or_else(|| "unknown error".to_string());
        return Err(ProviderError::fetch_failed_with_advice(
            FailureAdvice::ApiError { message: msg },
        ));
    }

    let model_remains = resp.model_remains.unwrap_or_default();
    if model_remains.is_empty() {
        return Err(ProviderError::no_data());
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
    use crate::providers::ProviderResult;

    #[test]
    fn invalid_json_is_classified_as_parse_failed() {
        let result: ProviderResult<Vec<QuotaInfo>> = parse_remains_response("not json");

        assert!(matches!(result, Err(ProviderError::ParseFailed { .. })));
    }

    #[test]
    fn api_error_is_classified_as_fetch_failed() {
        let result: ProviderResult<Vec<QuotaInfo>> =
            parse_remains_response(r#"{"base_resp":{"status_code":1001,"status_msg":"denied"}}"#);

        assert!(matches!(
            result,
            Err(ProviderError::FetchFailed {
                advice: Some(FailureAdvice::ApiError { message }),
                raw_detail: None,
            }) if message == "denied"
        ));
    }

    #[test]
    fn empty_model_remains_is_classified_as_no_data() {
        let result: ProviderResult<Vec<QuotaInfo>> =
            parse_remains_response(r#"{"base_resp":{"status_code":0},"model_remains":[]}"#);

        assert_eq!(result.unwrap_err(), ProviderError::NoData);
    }

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
