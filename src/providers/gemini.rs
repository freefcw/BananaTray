use super::{AiProvider, ProviderError};
use crate::models::{ProviderKind, ProviderMetadata, QuotaInfo, QuotaType, RefreshData};
use crate::utils::http_client;
use crate::utils::time_utils;
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use std::path::PathBuf;
use std::process::Command;

super::define_unit_provider!(GeminiProvider);

impl GeminiProvider {
    fn credentials_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("~"))
            .join(".gemini/oauth_creds.json")
    }

    fn settings_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("~"))
            .join(".gemini/settings.json")
    }

    /// Load OAuth credentials from ~/.gemini/oauth_creds.json
    fn load_credentials(&self) -> Result<OAuthCredentials> {
        let path = Self::credentials_path();
        let content = std::fs::read_to_string(&path)
            .map_err(|_| ProviderError::config_missing("~/.gemini/oauth_creds.json"))?;
        let creds: OAuthCredentials = serde_json::from_str(&content)
            .map_err(|_| ProviderError::parse_failed("oauth_creds.json"))?;
        Ok(creds)
    }

    /// Check if the auth type is supported (only oauth-personal is supported)
    fn check_auth_type(&self) -> Result<()> {
        let path = Self::settings_path();
        if let Ok(content) = std::fs::read_to_string(&path) {
            let settings: GeminiSettings = serde_json::from_str(&content)
                .map_err(|_| ProviderError::parse_failed("settings.json"))?;
            match settings.security.auth.selected_type.as_str() {
                "oauth-personal" | "unknown" => Ok(()),
                "api-key" => Err(ProviderError::config_missing(
                    "Gemini API key 不支持，请使用 Google 账户 (OAuth) 登录",
                )
                .into()),
                "vertex-ai" => Err(ProviderError::config_missing(
                    "Gemini Vertex AI 不支持，请使用 Google 账户 (OAuth) 登录",
                )
                .into()),
                _ => Ok(()),
            }
        } else {
            Ok(())
        }
    }

    /// Fetch quota from Google Cloud Code API
    fn fetch_quota_via_api(&self, access_token: &str) -> Result<Vec<QuotaInfo>> {
        let url = "https://cloudcode-pa.googleapis.com/v1internal:retrieveUserQuota";
        let auth_header = format!("Authorization: Bearer {}", access_token);

        let response_str =
            http_client::post_json(url, &[&auth_header, "Accept: application/json"], "{}")?;

        let response: QuotaResponse = serde_json::from_str(&response_str)
            .with_context(|| format!("Failed to parse API response: {}", response_str))?;

        // Group quotas by label instead of model_id, keeping lowest percentage per label
        let mut label_quotas: std::collections::HashMap<String, (f64, Option<String>)> =
            std::collections::HashMap::new();

        for bucket in response.buckets.unwrap_or_default() {
            if let (Some(model_id), Some(fraction)) = (bucket.model_id, bucket.remaining_fraction) {
                let percent_left = fraction * 100.0;
                let used_percent = 100.0 - percent_left;

                let label = Self::simplify_model_name(&model_id);

                // Keep the lowest percentage (most restrictive) per model label
                let entry = label_quotas
                    .entry(label)
                    .or_insert((used_percent, bucket.reset_time.clone()));
                if used_percent > entry.0 {
                    entry.0 = used_percent;
                    entry.1 = bucket.reset_time;
                }
            }
        }

        // Sort by label name and create QuotaInfo
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

    /// Fetch user info from Google userinfo API
    fn fetch_user_info(&self, access_token: &str) -> Option<String> {
        let url = "https://www.googleapis.com/oauth2/v2/userinfo";
        let auth_header = format!("Authorization: Bearer {}", access_token);

        let response_str =
            http_client::get(url, &[&auth_header, "Accept: application/json"]).ok()?;

        let user_info: UserInfo = serde_json::from_str(&response_str).ok()?;
        user_info.email
    }

    fn simplify_model_name(name: &str) -> String {
        // Convert "gemini-2.5-pro" -> "Pro", "gemini-2.5-flash" -> "Flash", etc.
        let lower = name.to_lowercase();
        if lower.contains("flash-lite") {
            "Flash Lite".to_string()
        } else if lower.contains("flash") {
            "Flash".to_string()
        } else if lower.contains("pro") {
            "Pro".to_string()
        } else {
            // Capitalize first letter of each word segment
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

    /// Try to refresh the OAuth token by briefly running the gemini CLI
    fn refresh_token_via_cli(&self) -> Result<()> {
        let output = Command::new("gemini").args(["--version"]).output();

        if output.is_err() {
            return Err(ProviderError::cli_not_found("gemini").into());
        }

        // Run `gemini` with `/quit` input to trigger token refresh without interactive session
        let output = Command::new("sh")
            .args(["-c", "echo '/quit' | gemini 2>/dev/null || true"])
            .output()
            .context("Failed to run gemini CLI for token refresh")?;

        if !output.status.success() {
            log::warn!(target: "providers", "gemini CLI token refresh exited with: {:?}", output.status);
        }

        // Wait briefly for the token file to be updated
        std::thread::sleep(std::time::Duration::from_millis(1500));

        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct OAuthCredentials {
    access_token: Option<String>,
    #[serde(rename = "expiry_date")]
    expiry_date_ms: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct GeminiSettings {
    security: GeminiSecurity,
}

#[derive(Debug, Deserialize)]
struct GeminiSecurity {
    auth: GeminiAuth,
}

#[derive(Debug, Deserialize)]
struct GeminiAuth {
    #[serde(rename = "selectedType")]
    selected_type: String,
}

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
struct UserInfo {
    email: Option<String>,
}

#[async_trait]
impl AiProvider for GeminiProvider {
    fn metadata(&self) -> ProviderMetadata {
        ProviderMetadata {
            kind: ProviderKind::Gemini,
            display_name: "Gemini".into(),
            brand_name: "Google".into(),
            icon_asset: "src/icons/provider-gemini.svg".into(),
            dashboard_url: "https://gemini.google.com".into(),
            account_hint: "Google account".into(),
            source_label: "gemini api".into(),
        }
    }

    fn id(&self) -> &'static str {
        "gemini:api"
    }

    async fn is_available(&self) -> bool {
        // Check if credentials file exists
        Self::credentials_path().exists()
    }

    async fn refresh(&self) -> Result<RefreshData> {
        // Check auth type first
        self.check_auth_type()?;

        // Load credentials
        let creds = self.load_credentials()?;

        let access_token = creds
            .access_token
            .filter(|t| !t.is_empty())
            .ok_or_else(|| ProviderError::auth_required(Some("请运行 `gemini` CLI 登录")))?;

        // Check if token is expired (expiry_date is in milliseconds since epoch)
        let token_expired = if let Some(expiry_ms) = creds.expiry_date_ms {
            let expiry_secs = expiry_ms / 1000.0;
            time_utils::is_expired_epoch_secs(expiry_secs)
        } else {
            false
        };

        if token_expired {
            // Try to refresh token via CLI
            log::info!(target: "providers", "Gemini token expired, attempting CLI refresh");
            if let Err(e) = self.refresh_token_via_cli() {
                log::warn!(target: "providers", "Gemini CLI token refresh failed: {e}");
                return Err(
                    ProviderError::session_expired(Some("请运行 `gemini` CLI 刷新")).into(),
                );
            }

            // Reload credentials after CLI refresh
            let refreshed_creds = self.load_credentials()?;
            let new_token = refreshed_creds
                .access_token
                .filter(|t| !t.is_empty())
                .ok_or_else(|| ProviderError::session_expired(Some("刷新后仍无有效 token")))?;

            let quotas = self.fetch_quota_via_api(&new_token)?;
            let account_email = self.fetch_user_info(&new_token);
            return Ok(RefreshData::with_account(quotas, account_email, None));
        }

        // Fetch quota via API
        match self.fetch_quota_via_api(&access_token) {
            Ok(quotas) => {
                let account_email = self.fetch_user_info(&access_token);
                Ok(RefreshData::with_account(quotas, account_email, None))
            }
            Err(e) => {
                let err_str = e.to_string();
                // If the error looks like an auth issue, try CLI refresh once
                if err_str.contains("401")
                    || err_str.contains("403")
                    || err_str.contains("Unauthorized")
                {
                    log::info!(target: "providers", "Gemini API returned auth error, attempting CLI refresh");
                    if self.refresh_token_via_cli().is_ok() {
                        let refreshed_creds = self.load_credentials()?;
                        if let Some(new_token) =
                            refreshed_creds.access_token.filter(|t| !t.is_empty())
                        {
                            let quotas = self.fetch_quota_via_api(&new_token)?;
                            let account_email = self.fetch_user_info(&new_token);
                            return Ok(RefreshData::with_account(quotas, account_email, None));
                        }
                    }
                }
                Err(e)
            }
        }
    }
}
