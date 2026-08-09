mod auth;
mod client;
mod parser;

use super::{
    AiProvider, ProviderCapabilities, ProviderError, ProviderExecutionContext, ProviderResult,
};
use crate::models::{
    AppSettings, ProviderDescriptor, ProviderKind, ProviderMetadata, RefreshData,
    SettingsCapability, TokenEditMode, TokenInputCapability, TokenInputState,
};
use async_trait::async_trait;
use log::debug;
use std::borrow::Cow;

#[cfg(test)]
use auth::{
    parse_providers_json, resolve_token, resolve_token_from_inputs, settings_path_from_sources,
    ClineTokenSource,
};
#[cfg(not(test))]
use auth::{resolve_token, ClineTokenSource};
#[cfg(test)]
use client::auth_header as cline_pass_auth_header;
use client::{fetch_usage, USAGE_URL};
use parser::parse_usage_response;

super::define_unit_provider!(ClinePassProvider);

fn cline_pass_settings_capability() -> SettingsCapability {
    SettingsCapability::TokenInput(TokenInputCapability {
        credential_key: "cline_api_key",
        placeholder_i18n_key: "cline_pass.token_placeholder",
        help_tip_i18n_key: "cline_pass.token_sources_tip",
        title_i18n_key: "cline_pass.api_key",
        description_i18n_key: "cline_pass.requires_auth",
        create_url: "https://app.cline.bot",
    })
}

#[async_trait]
impl AiProvider for ClinePassProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: Cow::Borrowed("cline-pass:api"),
            metadata: ProviderMetadata {
                kind: ProviderKind::ClinePass,
                display_name: "ClinePass".into(),
                brand_name: "Cline".into(),
                icon_asset: "src/icons/provider-cline-pass.svg".into(),
                dashboard_url: "https://app.cline.bot/dashboard/subscription?personal=true".into(),
                account_hint: "Cline account".into(),
                source_label: "cline api".into(),
            },
        }
    }

    async fn check_availability(&self, ctx: &ProviderExecutionContext<'_>) -> ProviderResult<()> {
        let configured_token = ctx.provider_credentials.get_credential("cline_api_key");
        let status = resolve_token(configured_token)?;
        debug!(
            target: "providers",
            "ClinePass availability: {} (token source: {})",
            status.token.is_some(),
            status.source.log_label()
        );
        if status.token.is_some() {
            Ok(())
        } else {
            Err(ProviderError::config_missing(
                "cline_api_key / CLINE_API_KEY / Cline login",
            ))
        }
    }

    async fn refresh(&self, ctx: &ProviderExecutionContext<'_>) -> ProviderResult<RefreshData> {
        let configured_token = ctx.provider_credentials.get_credential("cline_api_key");
        let status = resolve_token(configured_token)?;
        let token = status.token.ok_or_else(|| {
            ProviderError::config_missing("cline_api_key / CLINE_API_KEY / Cline login")
        })?;
        debug!(target: "providers", "cline-pass: fetching usage limits");
        let body = fetch_usage(USAGE_URL, &token)?;
        Ok(RefreshData::quotas_only(parse_usage_response(&body)?))
    }
}

impl ProviderCapabilities for ClinePassProvider {
    fn settings_capability(&self) -> SettingsCapability {
        cline_pass_settings_capability()
    }

    fn resolve_token_input_state(&self, settings: &AppSettings) -> Option<TokenInputState> {
        let SettingsCapability::TokenInput(config) = self.settings_capability() else {
            return None;
        };
        let status = resolve_token(
            settings
                .provider
                .credentials
                .get_credential(config.credential_key),
        )
        .ok();
        let source_i18n_key = status.as_ref().and_then(|status| match status.source {
            ClineTokenSource::ConfigFile if status.token.is_some() => {
                Some("cline_pass.source.config_file")
            }
            ClineTokenSource::EnvVar if status.token.is_some() => Some("cline_pass.source.env_var"),
            ClineTokenSource::LocalApiKey if status.token.is_some() => {
                Some("cline_pass.source.local_api_key")
            }
            ClineTokenSource::LocalOAuth if status.token.is_some() => {
                Some("cline_pass.source.cline_login")
            }
            _ => None,
        });
        let has_token = status.as_ref().is_some_and(|status| status.token.is_some());
        Some(TokenInputState {
            has_token,
            masked: status.as_ref().and_then(|status| status.masked()),
            source_i18n_key,
            edit_mode: if matches!(
                status.as_ref().map(|status| status.source),
                Some(ClineTokenSource::ConfigFile)
            ) && has_token
            {
                TokenEditMode::EditStored
            } else {
                TokenEditMode::SetNew
            },
        })
    }
}

#[cfg(test)]
mod tests;
