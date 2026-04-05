mod auth;
mod client;
mod parser;

use super::{AiProvider, ProviderError};
use crate::models::{ProviderDescriptor, ProviderKind, ProviderMetadata, RefreshData};
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::borrow::Cow;

use auth::{db_path, extract_user_id_from_jwt, read_access_token};
use client::fetch_usage_summary;
use parser::parse_usage_response;

super::define_unit_provider!(CursorProvider);

#[async_trait]
impl AiProvider for CursorProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: Cow::Borrowed("cursor:api"),
            metadata: ProviderMetadata {
                kind: ProviderKind::Cursor,
                display_name: "Cursor".into(),
                brand_name: "Cursor".into(),
                icon_asset: "src/icons/provider-cursor.svg".into(),
                dashboard_url: "https://cursor.com/dashboard?tab=usage".into(),
                account_hint: "Cursor account".into(),
                source_label: "cursor api".into(),
            },
        }
    }

    async fn check_availability(&self) -> Result<()> {
        if db_path().exists() {
            Ok(())
        } else {
            Err(ProviderError::config_missing(
                "~/Library/Application Support/Cursor/User/globalStorage/state.vscdb",
            )
            .into())
        }
    }

    async fn refresh(&self) -> Result<RefreshData> {
        let access_token = read_access_token().context("Failed to read Cursor access token")?;
        let user_id = extract_user_id_from_jwt(&access_token)
            .context("Failed to extract user ID from Cursor JWT")?;

        let cookie = format!("WorkosCursorSessionToken={}::{}", user_id, access_token);
        let body = fetch_usage_summary(&cookie).context("Failed to fetch Cursor usage summary")?;

        Ok(RefreshData::quotas_only(parse_usage_response(&body)?))
    }
}
