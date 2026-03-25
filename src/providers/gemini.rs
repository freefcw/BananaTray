use super::AiProvider;
use crate::models::{ProviderKind, QuotaInfo};
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

        // Group quotas by model, keeping lowest percentage per model
        let mut model_quotas: std::collections::HashMap<String, (f64, Option<String>)> =
            std::collections::HashMap::new();

        for bucket in response.buckets.unwrap_or_default() {
            if let (Some(model_id), Some(fraction)) = (bucket.model_id, bucket.remaining_fraction) {
                let percent_left = fraction * 100.0;
                let used_percent = 100.0 - percent_left;

                // Keep the lowest percentage (most restrictive) per model
                let entry = model_quotas
                    .entry(model_id)
                    .or_insert((used_percent, bucket.reset_time.clone()));
                if used_percent > entry.0 {
                    entry.0 = used_percent;
                    entry.1 = bucket.reset_time;
                }
            }
        }

        // Sort by model name and create QuotaInfo
        let mut quotas: Vec<QuotaInfo> = model_quotas
            .into_iter()
            .map(|(model_id, (used_percent, _reset))| {
                // Simplify model name for display
                let label = Self::simplify_model_name(&model_id);
                QuotaInfo::new(label, used_percent, 100.0)
            })
            .collect();

        quotas.sort_by(|a, b| a.label.cmp(&b.label));

        if quotas.is_empty() {
            anyhow::bail!("No quota data found in API response");
        }

        Ok(quotas)
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
        if let Some(expiry_ms) = creds.expiry_date_ms {
            let expiry_secs = expiry_ms / 1000.0;
            let now_secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs_f64())
                .unwrap_or(0.0);
            if expiry_secs < now_secs {
                anyhow::bail!("Gemini token expired. Please run 'gemini' CLI to refresh.");
            }
        }

        // Fetch quota via API
        self.fetch_quota_via_api(&access_token)
    }
}
