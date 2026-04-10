mod auth;
mod client;
mod parser;

use super::{AiProvider, ProviderError};
use crate::models::{ProviderDescriptor, ProviderKind, ProviderMetadata, RefreshData};
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::borrow::Cow;

use auth::{auth_path, get_valid_token, load_credentials, refresh_access_token};
use client::call_usage_api;
use parser::parse_usage_response;

super::define_unit_provider!(CodexProvider);

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
                let (_, refresh_token, _) = load_credentials()?;
                let new_token = refresh_access_token(&refresh_token)
                    .context(format!(
                        "API call failed ({}), and token refresh also failed. Run `codex` to re-authenticate.",
                        e
                    ))?;
                call_usage_api(&new_token)?
            }
        };

        Ok(RefreshData::quotas_only(parse_usage_response(&raw)?))
    }
}
