use super::AiProvider;
use crate::models::{ProviderKind, QuotaInfo, QuotaType};
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use std::path::PathBuf;
use std::process::Command;

pub struct GeminiProvider {}

impl GeminiProvider {
    pub fn new() -> Self {
        Self {}
    }

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
            .context("Failed to read ~/.gemini/oauth_creds.json. Are you logged in?")?;
        let creds: OAuthCredentials =
            serde_json::from_str(&content).context("Failed to parse oauth_creds.json")?;
        Ok(creds)
    }

    /// Check if the auth type is supported (only oauth-personal is supported)
    fn check_auth_type(&self) -> Result<()> {
        let path = Self::settings_path();
        if let Ok(content) = std::fs::read_to_string(&path) {
            let settings: GeminiSettings =
                serde_json::from_str(&content).context("Failed to parse settings.json")?;
            match settings.security.auth.selected_type.as_str() {
                "oauth-personal" | "unknown" => Ok(()),
                "api-key" => anyhow::bail!(
                    "Gemini API key auth not supported. Use Google account (OAuth) instead."
                ),
                "vertex-ai" => anyhow::bail!(
                    "Gemini Vertex AI auth not supported. Use Google account (OAuth) instead."
                ),
                _ => Ok(()),
            }
        } else {
            Ok(())
        }
    }

    /// Fetch quota from Google Cloud Code API
    fn fetch_quota_via_api(&self, access_token: &str) -> Result<Vec<QuotaInfo>> {
        let url = "https://cloudcode-pa.googleapis.com/v1internal:retrieveUserQuota";

        let output = Command::new("curl")
            .args([
                "-s",
                "-X",
                "POST",
                "-H",
                &format!("Authorization: Bearer {}", access_token),
                "-H",
                "Content-Type: application/json",
                "-H",
                "Accept: application/json",
                "-d",
                "{}",
                url,
            ])
            .output()
            .context("Failed to execute curl command")?;

        if !output.status.success() {
            anyhow::bail!("curl to Gemini API failed with status {:?}", output.status);
        }

        let response_str = String::from_utf8_lossy(&output.stdout);
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
                let reset_text = reset.as_ref().and_then(|r| Self::format_reset_time(r));
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
            anyhow::bail!("No quota data found in API response");
        }

        Ok(quotas)
    }

    /// Format ISO8601 reset time to human-readable countdown
    fn format_reset_time(iso_str: &str) -> Option<String> {
        // Parse ISO8601 timestamp like "2025-03-25T12:00:00Z" or "2025-03-25T12:00:00+00:00"
        // Extract date and time parts
        let clean = iso_str.trim_end_matches('Z');
        // Strip timezone offset if present (e.g. +08:00)
        let clean = if let Some(pos) = clean.rfind('+') {
            if pos > 10 {
                &clean[..pos]
            } else {
                clean
            }
        } else if let Some(pos) = clean.rfind('-') {
            // Avoid stripping the date separator; only strip if after index 10
            if pos > 10 {
                &clean[..pos]
            } else {
                clean
            }
        } else {
            clean
        };

        let parts: Vec<&str> = clean.split('T').collect();
        if parts.len() != 2 {
            return Some("Resets in ?".to_string());
        }

        let date_parts: Vec<&str> = parts[0].split('-').collect();
        let time_parts: Vec<&str> = parts[1].split(':').collect();
        if date_parts.len() != 3 || time_parts.len() < 2 {
            return Some("Resets in ?".to_string());
        }

        let year: i64 = date_parts[0].parse().unwrap_or(0);
        let month: i64 = date_parts[1].parse().unwrap_or(0);
        let day: i64 = date_parts[2].parse().unwrap_or(0);
        let hour: i64 = time_parts[0].parse().unwrap_or(0);
        let min: i64 = time_parts[1].parse().unwrap_or(0);

        // Rough epoch conversion (good enough for relative delta)
        let days_in_month = [0, 31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
        let mut total_days: i64 = 0;
        for y in 1970..year {
            total_days += if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 {
                366
            } else {
                365
            };
        }
        let is_leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
        for m in 1..month {
            total_days += days_in_month[m as usize];
            if m == 2 && is_leap {
                total_days += 1;
            }
        }
        total_days += day - 1;
        let reset_epoch_secs = total_days * 86400 + hour * 3600 + min * 60;

        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let delta = reset_epoch_secs - now_secs;
        if delta <= 0 {
            return Some("Resets soon".to_string());
        }

        let days = delta / 86400;
        let hours = (delta % 86400) / 3600;
        let mins = (delta % 3600) / 60;

        let text = if days > 0 {
            if hours > 0 {
                format!("Resets in {}d {}h", days, hours)
            } else {
                format!("Resets in {}d", days)
            }
        } else if hours > 0 {
            if mins > 0 {
                format!("Resets in {}h {}m", hours, mins)
            } else {
                format!("Resets in {}h", hours)
            }
        } else {
            format!("Resets in {}m", mins.max(1))
        };

        Some(text)
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
            anyhow::bail!("gemini CLI not found. Install it to enable automatic token refresh.");
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

#[async_trait]
impl AiProvider for GeminiProvider {
    fn id(&self) -> &'static str {
        "gemini:api"
    }

    fn kind(&self) -> ProviderKind {
        ProviderKind::Gemini
    }

    async fn is_available(&self) -> bool {
        // Check if credentials file exists
        Self::credentials_path().exists()
    }

    async fn refresh(&self) -> Result<Vec<QuotaInfo>> {
        // Check auth type first
        self.check_auth_type()?;

        // Load credentials
        let creds = self.load_credentials()?;

        let access_token = creds
            .access_token
            .filter(|t| !t.is_empty())
            .context("No access token found. Please login with 'gemini' CLI first.")?;

        // Check if token is expired (expiry_date is in milliseconds since epoch)
        let token_expired = if let Some(expiry_ms) = creds.expiry_date_ms {
            let expiry_secs = expiry_ms / 1000.0;
            let now_secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs_f64())
                .unwrap_or(0.0);
            expiry_secs < now_secs
        } else {
            false
        };

        if token_expired {
            // Try to refresh token via CLI
            log::info!(target: "providers", "Gemini token expired, attempting CLI refresh");
            if let Err(e) = self.refresh_token_via_cli() {
                log::warn!(target: "providers", "Gemini CLI token refresh failed: {e}");
                anyhow::bail!("Gemini token expired. Please run 'gemini' CLI to refresh. ({e})");
            }

            // Reload credentials after CLI refresh
            let refreshed_creds = self.load_credentials()?;
            let new_token = refreshed_creds
                .access_token
                .filter(|t| !t.is_empty())
                .context("Token still empty after CLI refresh.")?;

            return self.fetch_quota_via_api(&new_token);
        }

        // Fetch quota via API
        match self.fetch_quota_via_api(&access_token) {
            Ok(quotas) => Ok(quotas),
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
                            return self.fetch_quota_via_api(&new_token);
                        }
                    }
                }
                Err(e)
            }
        }
    }
}
