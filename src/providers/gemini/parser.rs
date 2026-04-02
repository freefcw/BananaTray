use crate::models::{QuotaInfo, QuotaType};
use crate::providers::common::jwt;
use crate::providers::ProviderError;
use crate::utils::time_utils;
use anyhow::{Context, Result};
use serde::Deserialize;

use super::auth::OAuthCredentials;

#[derive(Debug, Deserialize)]
struct QuotaResponse {
    buckets: Option<Vec<QuotaBucket>>,
}

#[derive(Debug, Deserialize)]
struct QuotaBucket {
    #[serde(rename = "remainingFraction")]
    remaining_fraction: Option<f64>,
    #[serde(rename = "modelId")]
    model_id: Option<String>,
    #[serde(rename = "resetTime")]
    reset_time: Option<String>,
}

#[derive(Debug, Deserialize)]
struct IdTokenClaims {
    email: Option<String>,
}

pub(super) fn parse_quota_response(response_str: &str) -> Result<Vec<QuotaInfo>> {
    let response: QuotaResponse = serde_json::from_str(response_str)
        .with_context(|| format!("Failed to parse API response: {}", response_str))?;

    let mut label_quotas: std::collections::HashMap<String, (f64, Option<String>)> =
        std::collections::HashMap::new();

    for bucket in response.buckets.unwrap_or_default() {
        if let (Some(model_id), Some(fraction)) = (bucket.model_id, bucket.remaining_fraction) {
            let percent_left = fraction * 100.0;
            let used_percent = 100.0 - percent_left;
            let label = simplify_model_name(&model_id);

            let entry = label_quotas
                .entry(label)
                .or_insert((used_percent, bucket.reset_time.clone()));
            if used_percent > entry.0 {
                entry.0 = used_percent;
                entry.1 = bucket.reset_time;
            }
        }
    }

    let mut quotas: Vec<QuotaInfo> = label_quotas
        .into_iter()
        .map(|(label, (used_percent, reset))| {
            let reset_text = reset
                .as_deref()
                .and_then(time_utils::format_reset_countdown);
            QuotaInfo::with_details(
                label.clone(),
                used_percent,
                100.0,
                QuotaType::ModelSpecific(label),
                reset_text,
            )
        })
        .collect();

    quotas.sort_by(|a, b| a.label.cmp(&b.label));

    if quotas.is_empty() {
        return Err(ProviderError::no_data().into());
    }

    Ok(quotas)
}

pub(super) fn extract_email_from_id_token(creds: &OAuthCredentials) -> Option<String> {
    let id_token = creds.id_token.as_deref()?;
    let claims: IdTokenClaims = jwt::decode_payload(id_token).ok()?;
    claims.email
}

pub(super) fn simplify_model_name(name: &str) -> String {
    let lower = name.to_lowercase();
    if lower.contains("flash-lite") {
        "Flash Lite".to_string()
    } else if lower.contains("flash") {
        "Flash".to_string()
    } else if lower.contains("pro") {
        "Pro".to_string()
    } else {
        name.split('-')
            .filter(|s| !s.is_empty())
            .map(|s| {
                let mut chars = s.chars();
                match chars.next() {
                    Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_quota_response_merges_same_label() {
        let body = r#"{
            "buckets": [
                {"modelId":"gemini-2.5-pro","remainingFraction":0.8,"resetTime":"2026-05-01T00:00:00Z"},
                {"modelId":"gemini-2.5-pro-preview","remainingFraction":0.3,"resetTime":"2026-05-02T00:00:00Z"},
                {"modelId":"gemini-2.5-flash","remainingFraction":0.6,"resetTime":"2026-05-03T00:00:00Z"}
            ]
        }"#;

        let quotas = parse_quota_response(body).unwrap();
        assert_eq!(quotas.len(), 2);
        assert_eq!(quotas[0].label, "Flash");
        assert_eq!(quotas[1].label, "Pro");
        assert_eq!(quotas[1].used, 70.0);
    }

    #[test]
    fn test_parse_quota_response_empty_returns_error() {
        let body = r#"{"buckets":[]}"#;
        assert!(parse_quota_response(body).is_err());
    }
}
