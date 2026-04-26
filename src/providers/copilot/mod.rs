mod client;
mod parser;
mod token;

use super::{AiProvider, ProviderError, ProviderResult};
use crate::models::{
    AppSettings, ProviderDescriptor, ProviderKind, ProviderMetadata, RefreshData,
    SettingsCapability, TokenEditMode, TokenInputCapability, TokenInputState,
};
use crate::providers::common::http_client::HttpError;
use anyhow::Context;
use async_trait::async_trait;
use log::debug;
use std::borrow::Cow;

use client::{fetch_github_user, fetch_user_info};
use parser::{parse_github_user, parse_user_info_response};
#[allow(unused_imports)]
pub use token::{resolve_token, CopilotTokenSource, CopilotTokenStatus};

super::define_unit_provider!(CopilotProvider);

pub(crate) fn copilot_settings_capability() -> SettingsCapability {
    SettingsCapability::TokenInput(TokenInputCapability {
        credential_key: "github_token",
        placeholder_i18n_key: "copilot.token_placeholder",
        help_tip_i18n_key: "copilot.token_sources_tip",
        title_i18n_key: "copilot.github_login",
        description_i18n_key: "copilot.requires_auth",
        create_url: "https://github.com/settings/personal-access-tokens",
    })
}

pub(crate) fn copilot_token_input_state(
    settings: &AppSettings,
    credential_key: &'static str,
) -> TokenInputState {
    let mem_token = settings.provider.credentials.get_credential(credential_key);
    let status = resolve_token(mem_token);

    let source_i18n_key = if status.token.is_some() {
        match status.source {
            CopilotTokenSource::ConfigFile => Some("copilot.source.config_file"),
            CopilotTokenSource::CopilotOAuth => Some("copilot.source.copilot_oauth"),
            CopilotTokenSource::CopilotCli => Some("copilot.source.copilot_cli"),
            CopilotTokenSource::EnvVar => Some("copilot.source.env_var"),
            CopilotTokenSource::None => None,
        }
    } else {
        None
    };

    TokenInputState {
        has_token: status.token.is_some(),
        masked: status.masked(),
        source_i18n_key,
        edit_mode: match status.source {
            CopilotTokenSource::ConfigFile if status.token.is_some() => TokenEditMode::EditStored,
            CopilotTokenSource::CopilotOAuth
            | CopilotTokenSource::CopilotCli
            | CopilotTokenSource::EnvVar
                if status.token.is_some() =>
            {
                TokenEditMode::SetNew
            }
            _ => TokenEditMode::SetNew,
        },
    }
}

#[async_trait]
impl AiProvider for CopilotProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: Cow::Borrowed("copilot:api"),
            metadata: ProviderMetadata {
                kind: ProviderKind::Copilot,
                display_name: "Copilot".into(),
                brand_name: "GitHub".into(),
                icon_asset: "src/icons/provider-copilot.svg".into(),
                dashboard_url: "https://github.com/settings/copilot/features".into(),
                account_hint: "GitHub account".into(),
                source_label: "github api".into(),
            },
        }
    }

    fn settings_capability(&self) -> SettingsCapability {
        copilot_settings_capability()
    }

    fn resolve_token_input_state(&self, settings: &AppSettings) -> Option<TokenInputState> {
        let SettingsCapability::TokenInput(config) = self.settings_capability() else {
            return None;
        };
        Some(copilot_token_input_state(settings, config.credential_key))
    }

    async fn check_availability(&self) -> ProviderResult<()> {
        let token_status = resolve_token(None);
        let available = token_status.token.is_some();
        debug!(
            target: "providers",
            "Copilot availability: {} (token source: {})",
            available,
            token_status.source.log_label()
        );
        if available {
            Ok(())
        } else {
            Err(ProviderError::config_missing("github_token / GITHUB_TOKEN"))
        }
    }

    async fn refresh(&self) -> ProviderResult<RefreshData> {
        let start = std::time::Instant::now();
        let token_status = resolve_token(None);

        let token = token_status.token.context(
            "GitHub token not configured. Set github_token in settings, or GITHUB_TOKEN environment variable.",
        )?;

        debug!(target: "providers", "copilot: fetching quota from api.github.com/copilot_internal/user");
        let body = match fetch_user_info(&token) {
            Ok(body) => body,
            Err(e) => {
                // 将 HTTP 状态码映射为用户可操作的提示
                if let Some(http_err) = e.downcast_ref::<HttpError>() {
                    match http_err {
                        HttpError::HttpStatus { code: 401, .. } => {
                            return Err(ProviderError::session_expired(Some(
                                crate::models::FailureAdvice::LoginApp {
                                    app: "GitHub".to_string(),
                                },
                            )));
                        }
                        HttpError::HttpStatus { code: 403, .. } => {
                            return Err(ProviderError::auth_required(Some(
                                crate::models::FailureAdvice::ApiError {
                                    message: "GitHub token lacks required Copilot permissions; use a Classic PAT with 'copilot' scope.".to_string(),
                                },
                            )));
                        }
                        HttpError::HttpStatus { code: 404, .. } => {
                            return Err(ProviderError::fetch_failed_with_advice(
                                crate::models::FailureAdvice::ApiError {
                                    message: "GitHub Copilot is not enabled for this account."
                                        .to_string(),
                                },
                            ));
                        }
                        _ => {}
                    }
                }
                return Err(e.into());
            }
        };
        debug!(target: "providers", "copilot: api response received in {:.2}s", start.elapsed().as_secs_f64());

        // /user API 获取账户标识（best-effort，失败不影响配额数据）
        let account_name = fetch_github_user(&token)
            .ok()
            .and_then(|user_body| parse_github_user(&user_body));

        Ok(parse_user_info_response(&body, account_name)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        ENV_LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn copilot_token_input_state_from_config_file_is_editable() {
        let _guard = env_lock().lock().unwrap();
        unsafe { std::env::remove_var("GITHUB_TOKEN") };
        crate::providers::copilot::token::set_test_cache(None, None);

        let mut settings = AppSettings::default();
        settings
            .provider
            .credentials
            .set_credential("github_token", "ghp_local_123456".to_string());

        let state = copilot_token_input_state(&settings, "github_token");

        assert!(state.has_token);
        assert_eq!(state.edit_mode, TokenEditMode::EditStored);
        assert_eq!(state.source_i18n_key, Some("copilot.source.config_file"));
        assert!(state.masked.is_some());
    }

    #[test]
    fn copilot_token_input_state_from_env_is_set_new() {
        let _guard = env_lock().lock().unwrap();
        unsafe { std::env::set_var("GITHUB_TOKEN", "ghp_env_123456") };
        crate::providers::copilot::token::set_test_cache(None, None);

        let state = copilot_token_input_state(&AppSettings::default(), "github_token");

        assert!(state.has_token);
        assert_eq!(state.edit_mode, TokenEditMode::SetNew);
        assert_eq!(state.source_i18n_key, Some("copilot.source.env_var"));

        unsafe { std::env::remove_var("GITHUB_TOKEN") };
    }

    #[test]
    fn copilot_token_input_state_without_any_source_is_empty() {
        let _guard = env_lock().lock().unwrap();
        unsafe { std::env::remove_var("GITHUB_TOKEN") };
        crate::providers::copilot::token::set_test_cache(None, None);

        let state = copilot_token_input_state(&AppSettings::default(), "github_token");

        assert!(!state.has_token);
        assert_eq!(state.edit_mode, TokenEditMode::SetNew);
        assert_eq!(state.source_i18n_key, None);
        assert_eq!(state.masked, None);
    }
}
