use super::{AiProvider, ProviderError};
use crate::models::{ProviderKind, ProviderMetadata, QuotaInfo, QuotaType};
use crate::utils::http_client;
use crate::utils::time_utils;
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;

super::define_unit_provider!(MiniMaxProvider);

impl MiniMaxProvider {
    fn get_api_key(&self) -> Option<String> {
        std::env::var("MINIMAX_API_KEY")
            .ok()
            .filter(|k| !k.is_empty())
    }

    fn api_url(&self) -> &'static str {
        let region = std::env::var("MINIMAX_REGION").unwrap_or_default();
        match region.to_lowercase().as_str() {
            "international" => "https://api.minimax.io/v1/api/openplatform/coding_plan/remains",
            _ => "https://api.minimaxi.com/v1/api/openplatform/coding_plan/remains",
        }
    }

    fn fetch_quota(&self, api_key: &str) -> Result<Vec<QuotaInfo>> {
        let auth_header = format!("Authorization: Bearer {}", api_key);

        let response_str = http_client::get(self.api_url(), &[&auth_header])?;

        let resp: MiniMaxRemainsResponse =
            serde_json::from_str(&response_str).with_context(|| {
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
            return Err(ProviderError::fetch_failed(&format!("API error: {}", msg)).into());
        }

        let model_remains = resp.model_remains.unwrap_or_default();
        if model_remains.is_empty() {
            return Err(ProviderError::no_data().into());
        }

        let quotas = model_remains
            .into_iter()
            .map(|model| {
                let total = model.current_interval_total_count;
                // ⚠️ MiniMax API naming is misleading:
                // "current_interval_usage_count" actually represents the REMAINING count.
                let remaining = model.current_interval_usage_count.clamp(0, total);
                let used = total - remaining;

                let reset_at = model.end_time.map(|ms| {
                    let epoch_secs = ms / 1000;
                    time_utils::format_reset_from_epoch(epoch_secs)
                });

                let label = model.model_name.clone();
                QuotaInfo::with_details(
                    &label,
                    used as f64,
                    total as f64,
                    QuotaType::ModelSpecific(model.model_name),
                    reset_at,
                )
            })
            .collect();

        Ok(quotas)
    }
}

#[async_trait]
impl AiProvider for MiniMaxProvider {
    fn metadata(&self) -> ProviderMetadata {
        ProviderMetadata {
            kind: ProviderKind::MiniMax,
            display_name: "MiniMax".into(),
            brand_name: "MiniMax".into(),
            icon_asset: "src/icons/provider-minimax.svg".into(),
            dashboard_url:
                "https://platform.minimax.io/user-center/payment/coding-plan?cycle_type=3".into(),
            account_hint: "MiniMax account".into(),
            source_label: "minimax api".into(),
        }
    }

    fn id(&self) -> &'static str {
        "minimax:api"
    }

    async fn is_available(&self) -> bool {
        self.get_api_key().is_some()
    }

    async fn refresh_quotas(&self) -> Result<Vec<QuotaInfo>> {
        let api_key = self
            .get_api_key()
            .ok_or_else(|| ProviderError::config_missing("MINIMAX_API_KEY"))?;

        self.fetch_quota(&api_key)
    }
}

// --- Serde structures ---

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
