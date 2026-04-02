mod auth;
mod client;
mod parser;

use super::{AiProvider, ProviderError};
use crate::models::{ProviderDescriptor, ProviderKind, ProviderMetadata, RefreshData};
use anyhow::Result;
use async_trait::async_trait;

use auth::{get_token, kimi_cli_exists};
use client::fetch_usage;
use parser::parse_usage_response;

super::define_unit_provider!(KimiProvider);

#[async_trait]
impl AiProvider for KimiProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: "kimi:api",
            metadata: ProviderMetadata {
                kind: ProviderKind::Kimi,
                display_name: "Kimi".into(),
                brand_name: "Moonshot".into(),
                icon_asset: "src/icons/provider-kimi.svg".into(),
                dashboard_url: "https://www.kimi.com/code/console".into(),
                account_hint: "Moonshot account".into(),
                source_label: "kimi api".into(),
            },
        }
    }

    async fn check_availability(&self) -> Result<()> {
        if get_token().is_some() {
            Ok(())
        } else if kimi_cli_exists() {
            Err(ProviderError::config_missing("KIMI_AUTH_TOKEN").into())
        } else {
            Err(ProviderError::cli_not_found("kimi").into())
        }
    }

    async fn refresh(&self) -> Result<RefreshData> {
        let token = get_token().ok_or_else(|| ProviderError::config_missing("KIMI_AUTH_TOKEN"))?;
        let body = fetch_usage(&token)?;
        Ok(RefreshData::quotas_only(parse_usage_response(&body)?))
    }
}
