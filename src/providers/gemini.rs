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

    /// Extract email from the id_token JWT in credentials (no network request needed)
    fn extract_email_from_id_token(creds: &OAuthCredentials) -> Option<String> {
        let id_token = creds.id_token.as_deref()?;
        let parts: Vec<&str> = id_token.split('.').collect();
        if parts.len() < 2 {
            return None;
        }

        use base64::Engine;
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(parts[1])
            .ok()?;
        let claims: IdTokenClaims = serde_json::from_slice(&payload).ok()?;
        claims.email
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

    /// Load fresh credentials, extract email, fetch quota, and return RefreshData.
    ///
    /// Shared by all refresh paths (normal, token-expired, auth-error-retry)
    /// to avoid duplicating the creds→email→quota→RefreshData pipeline.
    fn fetch_quota_from_current_creds(
        &self,
        fallback_email: Option<String>,
    ) -> Result<RefreshData> {
        let creds = self.load_credentials()?;
        let email = Self::extract_email_from_id_token(&creds).or(fallback_email);
        let token = creds
            .access_token
            .filter(|t| !t.is_empty())
            .ok_or_else(|| ProviderError::session_expired(Some("刷新后仍无有效 token")))?;
        let quotas = self.fetch_quota_via_api(&token)?;
        Ok(RefreshData::with_account(quotas, email, None))
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

        // Poll for token file update instead of fixed sleep
        let creds_path = Self::credentials_path();
        let before = std::fs::metadata(&creds_path)
            .and_then(|m| m.modified())
            .ok();

        for _ in 0..10 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            let after = std::fs::metadata(&creds_path)
                .and_then(|m| m.modified())
                .ok();
            if after != before {
                return Ok(());
            }
        }

        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct OAuthCredentials {
    access_token: Option<String>,
    #[serde(rename = "expiry_date")]
    expiry_date_ms: Option<f64>,
    id_token: Option<String>,
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
struct IdTokenClaims {
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
        self.check_auth_type()?;

        let creds = self.load_credentials()?;

        let access_token = creds
            .access_token
            .as_deref()
            .filter(|t| !t.is_empty())
            .ok_or_else(|| ProviderError::auth_required(Some("请运行 `gemini` CLI 登录")))?
            .to_string();

        let account_email = Self::extract_email_from_id_token(&creds);

        // Check if token is expired (expiry_date is in milliseconds since epoch)
        let token_expired = creds
            .expiry_date_ms
            .is_some_and(|ms| time_utils::is_expired_epoch_secs(ms / 1000.0));

        if token_expired {
            log::info!(target: "providers", "Gemini token expired, attempting CLI refresh");
            self.refresh_token_via_cli().map_err(|e| {
                log::warn!(target: "providers", "Gemini CLI token refresh failed: {e}");
                ProviderError::session_expired(Some("请运行 `gemini` CLI 刷新"))
            })?;
            return self.fetch_quota_from_current_creds(account_email);
        }

        // Normal path: try API directly, fall back to CLI refresh on auth errors
        match self.fetch_quota_via_api(&access_token) {
            Ok(quotas) => Ok(RefreshData::with_account(quotas, account_email, None)),
            Err(e) => {
                let err_str = e.to_string();
                let is_auth_error = err_str.contains("401")
                    || err_str.contains("403")
                    || err_str.contains("Unauthorized");

                if is_auth_error {
                    log::info!(target: "providers", "Gemini API returned auth error, attempting CLI refresh");
                    if self.refresh_token_via_cli().is_ok() {
                        return self.fetch_quota_from_current_creds(account_email);
                    }
                }
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- extract_email_from_id_token ---

    fn make_creds_with_id_token(id_token: Option<&str>) -> OAuthCredentials {
        OAuthCredentials {
            access_token: Some("test_token".to_string()),
            expiry_date_ms: Some(9999999999000.0),
            id_token: id_token.map(|s| s.to_string()),
        }
    }

    fn make_jwt(payload_json: &str) -> String {
        use base64::Engine;
        let header = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(r#"{"alg":"RS256","typ":"JWT"}"#);
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload_json);
        format!("{}.{}.fake_signature", header, payload)
    }

    #[test]
    fn test_extract_email_valid_jwt() {
        let jwt = make_jwt(r#"{"email":"user@example.com","sub":"123"}"#);
        let creds = make_creds_with_id_token(Some(&jwt));
        assert_eq!(
            GeminiProvider::extract_email_from_id_token(&creds),
            Some("user@example.com".to_string())
        );
    }

    #[test]
    fn test_extract_email_no_email_in_jwt() {
        let jwt = make_jwt(r#"{"sub":"123"}"#);
        let creds = make_creds_with_id_token(Some(&jwt));
        assert_eq!(GeminiProvider::extract_email_from_id_token(&creds), None);
    }

    #[test]
    fn test_extract_email_no_id_token() {
        let creds = make_creds_with_id_token(None);
        assert_eq!(GeminiProvider::extract_email_from_id_token(&creds), None);
    }

    #[test]
    fn test_extract_email_invalid_jwt_format() {
        let creds = make_creds_with_id_token(Some("not.a.valid.jwt.at.all"));
        // Should return None gracefully, not panic
        let _ = GeminiProvider::extract_email_from_id_token(&creds);
    }

    #[test]
    fn test_extract_email_single_segment() {
        let creds = make_creds_with_id_token(Some("no_dots_here"));
        assert_eq!(GeminiProvider::extract_email_from_id_token(&creds), None);
    }

    // --- simplify_model_name ---

    #[test]
    fn test_simplify_pro() {
        assert_eq!(GeminiProvider::simplify_model_name("gemini-2.5-pro"), "Pro");
        assert_eq!(
            GeminiProvider::simplify_model_name("gemini-2.0-pro-exp"),
            "Pro"
        );
    }

    #[test]
    fn test_simplify_flash() {
        assert_eq!(
            GeminiProvider::simplify_model_name("gemini-2.5-flash"),
            "Flash"
        );
    }

    #[test]
    fn test_simplify_flash_lite() {
        assert_eq!(
            GeminiProvider::simplify_model_name("gemini-2.0-flash-lite"),
            "Flash Lite"
        );
    }

    #[test]
    fn test_simplify_unknown_model() {
        assert_eq!(
            GeminiProvider::simplify_model_name("gemini-3.0-ultra"),
            "Gemini 3.0 Ultra"
        );
    }

    // --- check_auth_type (using filesystem, test with missing file) ---

    #[test]
    fn test_check_auth_type_missing_file_ok() {
        // When settings file doesn't exist, should succeed (permissive default)
        let provider = GeminiProvider::new();
        assert!(provider.check_auth_type().is_ok());
    }
}
