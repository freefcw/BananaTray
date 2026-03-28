use super::AiProvider;
use crate::models::{ProviderKind, ProviderMetadata, QuotaInfo, QuotaType};
use crate::utils::http_client;
use crate::utils::time_utils;
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use std::path::PathBuf;

pub struct CodexProvider {}

impl Default for CodexProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl CodexProvider {
    pub fn new() -> Self {
        Self {}
    }

    fn auth_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".codex")
            .join("auth.json")
    }

    /// Load and return (access_token, refresh_token, last_refresh) from ~/.codex/auth.json
    fn load_credentials() -> Result<(String, String, Option<String>)> {
        let path = Self::auth_path();
        let content = std::fs::read_to_string(&path)
            .context("Codex CLI not configured. Run `codex` to authenticate.")?;
        let json: serde_json::Value =
            serde_json::from_str(&content).context("Failed to parse ~/.codex/auth.json")?;

        let tokens = json
            .get("tokens")
            .context("No 'tokens' field in auth.json")?;

        let access_token = tokens
            .get("access_token")
            .and_then(|v| v.as_str())
            .context("No access_token in auth.json")?
            .to_string();

        let refresh_token = tokens
            .get("refresh_token")
            .and_then(|v| v.as_str())
            .context("No refresh_token in auth.json")?
            .to_string();

        let last_refresh = json
            .get("last_refresh")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        Ok((access_token, refresh_token, last_refresh))
    }

    /// Check if the token is older than 8 days and needs refresh.
    fn token_needs_refresh(last_refresh: &Option<String>) -> bool {
        let Some(ts) = last_refresh else {
            return true;
        };

        let eight_days_secs: i64 = 8 * 24 * 60 * 60;
        time_utils::is_older_than(ts, eight_days_secs)
    }

    /// Refresh the OAuth token via OpenAI's auth endpoint.
    /// Returns the new access token on success and updates ~/.codex/auth.json.
    fn do_token_refresh(refresh_token: &str) -> Result<String> {
        let body = format!(
            "grant_type=refresh_token&refresh_token={}&client_id=app_EMoamEEZ73f0CkXaXp7hrann",
            refresh_token
        );

        let response_str =
            http_client::post_form("https://auth.openai.com/oauth/token", &[], &body)?;

        let resp: serde_json::Value = serde_json::from_str(&response_str)
            .context("Failed to parse token refresh response")?;

        let new_access = resp
            .get("access_token")
            .and_then(|v| v.as_str())
            .context("No access_token in refresh response")?
            .to_string();

        let new_refresh = resp
            .get("refresh_token")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Update auth.json on disk
        Self::save_refreshed_tokens(&new_access, new_refresh.as_deref(), refresh_token)?;

        Ok(new_access)
    }

    /// Persist updated tokens back to ~/.codex/auth.json.
    fn save_refreshed_tokens(
        access_token: &str,
        new_refresh_token: Option<&str>,
        old_refresh_token: &str,
    ) -> Result<()> {
        let path = Self::auth_path();
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        let mut json: serde_json::Value =
            serde_json::from_str(&content).unwrap_or(serde_json::json!({}));

        if let Some(tokens) = json.get_mut("tokens") {
            tokens["access_token"] = serde_json::json!(access_token);
            if let Some(rt) = new_refresh_token {
                tokens["refresh_token"] = serde_json::json!(rt);
            }
        } else {
            json["tokens"] = serde_json::json!({
                "access_token": access_token,
                "refresh_token": new_refresh_token.unwrap_or(old_refresh_token),
            });
        }

        let now_str = time_utils::epoch_to_iso8601(time_utils::now_epoch_secs() as u64);
        json["last_refresh"] = serde_json::json!(now_str);

        let serialized = serde_json::to_string_pretty(&json)?;
        std::fs::write(&path, serialized).context("Failed to write updated auth.json")?;

        Ok(())
    }

    /// Get a valid access token, refreshing if necessary.
    fn get_valid_token() -> Result<String> {
        let (access_token, refresh_token, last_refresh) = Self::load_credentials()?;

        if Self::token_needs_refresh(&last_refresh) {
            match Self::do_token_refresh(&refresh_token) {
                Ok(new_token) => Ok(new_token),
                Err(_) => {
                    // Token might still work even if refresh fails
                    Ok(access_token)
                }
            }
        } else {
            Ok(access_token)
        }
    }

    /// Call the ChatGPT backend API and return raw response (headers + body).
    fn call_usage_api(access_token: &str) -> Result<String> {
        let auth_header = format!("Authorization: Bearer {}", access_token);
        http_client::get_with_headers(
            "https://chatgpt.com/backend-api/wham/usage",
            &[
                &auth_header,
                "Accept: application/json",
                "User-Agent: OpenUsage",
            ],
        )
    }

    /// Parse quota information from the API response (headers + body).
    fn parse_usage_response(raw: &str) -> Result<Vec<QuotaInfo>> {
        let mut quotas = Vec::new();

        // Split headers from body (separated by \r\n\r\n or \n\n)
        let (headers, body) = if let Some(idx) = raw.find("\r\n\r\n") {
            (&raw[..idx], raw[idx + 4..].trim())
        } else if let Some(idx) = raw.find("\n\n") {
            (&raw[..idx], raw[idx + 2..].trim())
        } else {
            ("", raw.trim())
        };

        // Check for 401/403 in the status line
        let first_line = headers.lines().next().unwrap_or("");
        if first_line.contains("401") || first_line.contains("403") {
            bail!("Token expired. Run `codex` to re-authenticate.");
        }

        // Try parsing from custom headers first
        let mut found_headers = false;
        let mut primary_percent: Option<f64> = None;
        let mut secondary_percent: Option<f64> = None;
        let mut credits_balance: Option<f64> = None;

        for line in headers.lines() {
            let lower = line.to_lowercase();
            if lower.starts_with("x-codex-primary-used-percent:") {
                primary_percent = line
                    .split_once(':')
                    .and_then(|(_, v)| v.trim().parse::<f64>().ok());
                found_headers = true;
            } else if lower.starts_with("x-codex-secondary-used-percent:") {
                secondary_percent = line
                    .split_once(':')
                    .and_then(|(_, v)| v.trim().parse::<f64>().ok());
                found_headers = true;
            } else if lower.starts_with("x-codex-credits-balance:") {
                credits_balance = line
                    .split_once(':')
                    .and_then(|(_, v)| v.trim().parse::<f64>().ok());
                found_headers = true;
            }
        }

        if found_headers {
            if let Some(primary) = primary_percent {
                quotas.push(QuotaInfo::with_details(
                    "Session (5h)",
                    primary,
                    100.0,
                    QuotaType::Session,
                    None,
                ));
            }
            if let Some(secondary) = secondary_percent {
                quotas.push(QuotaInfo::with_details(
                    "Weekly",
                    secondary,
                    100.0,
                    QuotaType::Weekly,
                    None,
                ));
            }
            if let Some(credits) = credits_balance {
                quotas.push(QuotaInfo::with_details(
                    "Credits",
                    0.0,
                    credits,
                    QuotaType::Credit,
                    None,
                ));
            }
            return Ok(quotas);
        }

        // Fall back to JSON body parsing
        if body.is_empty() {
            bail!("No usage data available from Codex API.");
        }

        let json: serde_json::Value =
            serde_json::from_str(body).context("Failed to parse Codex usage API response")?;

        if let Some(rate_limit) = json.get("rate_limit") {
            if let Some(primary) = rate_limit.get("primary_window") {
                let used = primary
                    .get("used_percent")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let reset_at = primary.get("reset_at").and_then(|v| v.as_i64());

                quotas.push(QuotaInfo::with_details(
                    "Session (5h)",
                    used,
                    100.0,
                    QuotaType::Session,
                    reset_at.map(time_utils::format_reset_from_epoch),
                ));
            }

            if let Some(secondary) = rate_limit.get("secondary_window") {
                let used = secondary
                    .get("used_percent")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let reset_at = secondary.get("reset_at").and_then(|v| v.as_i64());

                quotas.push(QuotaInfo::with_details(
                    "Weekly",
                    used,
                    100.0,
                    QuotaType::Weekly,
                    reset_at.map(time_utils::format_reset_from_epoch),
                ));
            }
        }

        if quotas.is_empty() {
            bail!("No usage data available from Codex API.");
        }

        Ok(quotas)
    }
}

#[async_trait]
impl AiProvider for CodexProvider {
    fn metadata(&self) -> ProviderMetadata {
        ProviderMetadata {
            kind: ProviderKind::Codex,
            display_name: "Codex".into(),
            brand_name: "OpenAI".into(),
            icon_asset: "src/icons/provider-codex.svg".into(),
            dashboard_url: "https://platform.openai.com/usage".into(),
            account_hint: "OpenAI account".into(),
            source_label: "openai api".into(),
        }
    }

    fn id(&self) -> &'static str {
        "codex:api"
    }

    async fn is_available(&self) -> bool {
        Self::auth_path().exists()
    }

    async fn refresh(&self) -> Result<Vec<QuotaInfo>> {
        let access_token = Self::get_valid_token()?;

        let raw = match Self::call_usage_api(&access_token) {
            Ok(r) => r,
            Err(e) => {
                // If the first attempt fails, try refreshing the token
                let (_, refresh_token, _) = Self::load_credentials()?;
                let new_token = Self::do_token_refresh(&refresh_token)
                    .context(format!("API call failed ({}), and token refresh also failed. Run `codex` to re-authenticate.", e))?;
                Self::call_usage_api(&new_token)?
            }
        };

        Self::parse_usage_response(&raw)
    }
}
