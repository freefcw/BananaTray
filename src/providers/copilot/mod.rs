mod client;
mod parser;
pub mod settings_ui;
mod token;

use super::{AiProvider, ProviderError};
use crate::models::{ProviderDescriptor, ProviderKind, ProviderMetadata, RefreshData};
use anyhow::{Context, Result};
use async_trait::async_trait;
use log::debug;

use client::fetch_user_info;
use parser::parse_user_info_response;
#[allow(unused_imports)]
pub use token::{resolve_token, CopilotTokenSource, CopilotTokenStatus};

super::define_unit_provider!(CopilotProvider);

#[async_trait]
impl AiProvider for CopilotProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: "copilot:api",
            metadata: ProviderMetadata {
                kind: ProviderKind::Copilot,
                display_name: "Copilot".into(),
                brand_name: "GitHub".into(),
                icon_asset: "src/icons/provider-copilot.svg".into(),
                dashboard_url: "https://github.com/settings/copilot".into(),
                account_hint: "GitHub account".into(),
                source_label: "github api".into(),
            },
        }
    }

    async fn check_availability(&self) -> Result<()> {
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
            Err(ProviderError::config_missing("github_token / GITHUB_TOKEN").into())
        }
    }

    async fn refresh(&self) -> Result<RefreshData> {
        let token_status = resolve_token(None);

        let token = token_status.token.context(
            "GitHub token not configured. Set github_token in settings, or GITHUB_TOKEN environment variable.",
        )?;

        let (body, status_code) = fetch_user_info(&token)?;
        parse_user_info_response(&body, &status_code)
    }
}
