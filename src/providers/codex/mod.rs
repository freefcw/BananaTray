mod auth;
mod client;
mod parser;

use super::{AiProvider, ProviderError};
use crate::models::{
    FailureAdvice, ProviderDescriptor, ProviderKind, ProviderMetadata, RefreshData,
};
use crate::providers::common::http_client::HttpError;
use anyhow::Result;
use async_trait::async_trait;
use std::borrow::Cow;

use auth::{auth_path, get_valid_token, load_credentials, refresh_access_token};
use client::call_usage_api;
use parser::parse_usage_response;

super::define_unit_provider!(CodexProvider);

fn is_auth_api_error(err: &anyhow::Error) -> bool {
    err.downcast_ref::<HttpError>()
        .is_some_and(|http_error| http_error.is_auth_error())
}

fn auth_refresh_failed_error(
    usage_error: &anyhow::Error,
    refresh_error: &anyhow::Error,
) -> ProviderError {
    log::warn!(
        target: "providers",
        "Codex usage API returned auth error: {usage_error}"
    );
    log::warn!(
        target: "providers",
        "Codex token refresh failed: {refresh_error}"
    );
    ProviderError::session_expired(Some(FailureAdvice::ReloginCli {
        cli: "codex".to_string(),
    }))
}

#[async_trait]
impl AiProvider for CodexProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: Cow::Borrowed("codex:api"),
            metadata: ProviderMetadata {
                kind: ProviderKind::Codex,
                display_name: "Codex".into(),
                brand_name: "OpenAI".into(),
                icon_asset: "src/icons/provider-codex.svg".into(),
                dashboard_url: "https://platform.openai.com/usage".into(),
                account_hint: "OpenAI account".into(),
                source_label: "openai api".into(),
            },
        }
    }

    async fn check_availability(&self) -> Result<()> {
        if auth_path().exists() {
            Ok(())
        } else {
            Err(ProviderError::config_missing("~/.codex/auth.json").into())
        }
    }

    async fn refresh(&self) -> Result<RefreshData> {
        let access_token = get_valid_token()?;

        let raw = match call_usage_api(&access_token) {
            Ok(r) => r,
            Err(e) => {
                // 只有认证类错误才尝试刷新 token，429/5xx 等不应触发 OAuth refresh
                if !is_auth_api_error(&e) {
                    return Err(e);
                }
                let (_, refresh_token, _) = load_credentials()?;
                let new_token = match refresh_access_token(&refresh_token) {
                    Ok(token) => token,
                    Err(refresh_err) => {
                        return Err(auth_refresh_failed_error(&e, &refresh_err).into());
                    }
                };
                call_usage_api(&new_token)?
            }
        };

        Ok(RefreshData::quotas_only(parse_usage_response(&raw)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_auth_error_detection_only_matches_401_and_403() {
        let auth_401: anyhow::Error = HttpError::HttpStatus {
            code: 401,
            body: "Unauthorized".into(),
        }
        .into();
        let auth_403: anyhow::Error = HttpError::HttpStatus {
            code: 403,
            body: "Forbidden".into(),
        }
        .into();
        let other_status: anyhow::Error = HttpError::HttpStatus {
            code: 500,
            body: "Server Error".into(),
        }
        .into();

        assert!(is_auth_api_error(&auth_401));
        assert!(is_auth_api_error(&auth_403));
        assert!(!is_auth_api_error(&other_status));
    }

    #[test]
    fn codex_refresh_failure_returns_structured_auth_error() {
        let usage_error: anyhow::Error = HttpError::HttpStatus {
            code: 401,
            body: "Unauthorized".into(),
        }
        .into();
        let refresh_error = anyhow::anyhow!("refresh token rejected");

        let provider_error = auth_refresh_failed_error(&usage_error, &refresh_error);

        assert!(matches!(
            provider_error,
            ProviderError::SessionExpired {
                advice: Some(FailureAdvice::ReloginCli { ref cli })
            } if cli == "codex"
        ));
    }
}
