mod auth;
mod client;
mod parser;

use super::{AiProvider, ProviderError};
use crate::models::{ProviderDescriptor, ProviderKind, ProviderMetadata, RefreshData};
use crate::utils::time_utils;
use anyhow::Result;
use async_trait::async_trait;
use rust_i18n::t;

use auth::{check_auth_type, credentials_path, load_credentials, refresh_token_via_cli};
use client::fetch_quota_via_api;
use parser::extract_email_from_id_token;

super::define_unit_provider!(GeminiProvider);

impl GeminiProvider {
    fn fetch_quota_from_current_creds(
        &self,
        fallback_email: Option<String>,
    ) -> Result<RefreshData> {
        let creds = load_credentials()?;
        let email = extract_email_from_id_token(&creds).or(fallback_email);
        let token = creds
            .access_token
            .filter(|t| !t.is_empty())
            .ok_or_else(|| ProviderError::session_expired(Some(&t!("hint.token_still_invalid"))))?;
        let quotas = fetch_quota_via_api(&token)?;
        Ok(RefreshData::with_account(quotas, email, None))
    }
}

#[async_trait]
impl AiProvider for GeminiProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: "gemini:api",
            metadata: ProviderMetadata {
                kind: ProviderKind::Gemini,
                display_name: "Gemini".into(),
                brand_name: "Google".into(),
                icon_asset: "src/icons/provider-gemini.svg".into(),
                dashboard_url: "https://gemini.google.com".into(),
                account_hint: "Google account".into(),
                source_label: "gemini api".into(),
            },
        }
    }

    async fn check_availability(&self) -> Result<()> {
        if credentials_path().exists() {
            Ok(())
        } else {
            Err(ProviderError::config_missing("~/.gemini/oauth_creds.json").into())
        }
    }

    async fn refresh(&self) -> Result<RefreshData> {
        check_auth_type()?;

        let creds = load_credentials()?;

        let access_token = creds
            .access_token
            .as_deref()
            .filter(|t| !t.is_empty())
            .ok_or_else(|| {
                ProviderError::auth_required(Some(&t!("hint.login_cli", cli = "gemini")))
            })?
            .to_string();

        let account_email = extract_email_from_id_token(&creds);

        let token_expired = creds
            .expiry_date_ms
            .is_some_and(|ms| time_utils::is_expired_epoch_secs(ms / 1000.0));

        if token_expired {
            log::info!(target: "providers", "Gemini token expired, attempting CLI refresh");
            refresh_token_via_cli().map_err(|e| {
                log::warn!(target: "providers", "Gemini CLI token refresh failed: {e}");
                ProviderError::session_expired(Some(&t!("hint.refresh_cli", cli = "gemini")))
            })?;
            return self.fetch_quota_from_current_creds(account_email);
        }

        match fetch_quota_via_api(&access_token) {
            Ok(quotas) => Ok(RefreshData::with_account(quotas, account_email, None)),
            Err(e) => {
                let err_str = e.to_string();
                let is_auth_error = err_str.contains("status 401")
                    || err_str.contains("status 403")
                    || err_str.contains("Unauthorized");

                if is_auth_error {
                    log::info!(target: "providers", "Gemini API returned auth error, attempting CLI refresh");
                    if refresh_token_via_cli().is_ok() {
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
    use super::auth::{check_auth_type, OAuthCredentials};
    use super::parser::{extract_email_from_id_token, simplify_model_name};

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
            extract_email_from_id_token(&creds),
            Some("user@example.com".to_string())
        );
    }

    #[test]
    fn test_extract_email_no_email_in_jwt() {
        let jwt = make_jwt(r#"{"sub":"123"}"#);
        let creds = make_creds_with_id_token(Some(&jwt));
        assert_eq!(extract_email_from_id_token(&creds), None);
    }

    #[test]
    fn test_extract_email_no_id_token() {
        let creds = make_creds_with_id_token(None);
        assert_eq!(extract_email_from_id_token(&creds), None);
    }

    #[test]
    fn test_extract_email_invalid_jwt_format() {
        let creds = make_creds_with_id_token(Some("not.a.valid.jwt.at.all"));
        let _ = extract_email_from_id_token(&creds);
    }

    #[test]
    fn test_extract_email_single_segment() {
        let creds = make_creds_with_id_token(Some("no_dots_here"));
        assert_eq!(extract_email_from_id_token(&creds), None);
    }

    #[test]
    fn test_simplify_pro() {
        assert_eq!(simplify_model_name("gemini-2.5-pro"), "Pro");
        assert_eq!(simplify_model_name("gemini-2.0-pro-exp"), "Pro");
    }

    #[test]
    fn test_simplify_flash() {
        assert_eq!(simplify_model_name("gemini-2.5-flash"), "Flash");
    }

    #[test]
    fn test_simplify_flash_lite() {
        assert_eq!(simplify_model_name("gemini-2.0-flash-lite"), "Flash Lite");
    }

    #[test]
    fn test_simplify_unknown_model() {
        assert_eq!(simplify_model_name("gemini-3.0-ultra"), "Gemini 3.0 Ultra");
    }

    #[test]
    fn test_check_auth_type_missing_file_ok() {
        assert!(check_auth_type().is_ok());
    }

    #[test]
    fn test_check_auth_type_oauth_accepted() {
        use super::auth::check_auth_type_from_content;
        let json = r#"{"security":{"auth":{"selectedType":"oauth-personal"}}}"#;
        assert!(check_auth_type_from_content(json).is_ok());
    }

    #[test]
    fn test_check_auth_type_api_key_rejected() {
        use super::auth::check_auth_type_from_content;
        let json = r#"{"security":{"auth":{"selectedType":"api-key"}}}"#;
        let err = check_auth_type_from_content(json).unwrap_err();
        let classified = crate::providers::ProviderError::classify(&err);
        assert!(matches!(
            classified,
            crate::providers::ProviderError::ConfigMissing { .. }
        ));
    }

    #[test]
    fn test_check_auth_type_vertex_ai_rejected() {
        use super::auth::check_auth_type_from_content;
        let json = r#"{"security":{"auth":{"selectedType":"vertex-ai"}}}"#;
        let err = check_auth_type_from_content(json).unwrap_err();
        let classified = crate::providers::ProviderError::classify(&err);
        assert!(matches!(
            classified,
            crate::providers::ProviderError::ConfigMissing { .. }
        ));
    }
}
