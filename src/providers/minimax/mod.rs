mod auth;
mod client;
mod parser;

use super::{AiProvider, ProviderError};
use crate::models::{ProviderDescriptor, ProviderKind, ProviderMetadata, RefreshData};
use anyhow::Result;
use async_trait::async_trait;
use std::borrow::Cow;

use auth::{api_url, get_api_key};
use client::fetch_remains;
use parser::parse_remains_response;

super::define_unit_provider!(MiniMaxProvider);

#[async_trait]
impl AiProvider for MiniMaxProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: Cow::Borrowed("minimax:api"),
            metadata: ProviderMetadata {
                kind: ProviderKind::MiniMax,
                display_name: "MiniMax".into(),
                brand_name: "MiniMax".into(),
                icon_asset: "src/icons/provider-minimax.svg".into(),
                dashboard_url:
                    "https://platform.minimax.io/user-center/payment/coding-plan?cycle_type=3"
                        .into(),
                account_hint: "MiniMax account".into(),
                source_label: "minimax api".into(),
            },
        }
    }

    async fn check_availability(&self) -> Result<()> {
        if get_api_key().is_some() {
            Ok(())
        } else {
            Err(ProviderError::config_missing("MINIMAX_API_KEY").into())
        }
    }

    async fn refresh(&self) -> Result<RefreshData> {
        let api_key =
            get_api_key().ok_or_else(|| ProviderError::config_missing("MINIMAX_API_KEY"))?;
        let body = fetch_remains(api_url(), &api_key)?;
        Ok(RefreshData::quotas_only(parse_remains_response(&body)?))
    }
}
