use super::{AiProvider, ProviderError};
use crate::models::{ProviderDescriptor, ProviderKind, ProviderMetadata, RefreshData};
use crate::providers::common::cli;
use anyhow::Result;
use async_trait::async_trait;

const OPENCODE_CLI: &str = "opencode";

super::define_unit_provider!(OpenCodeProvider);

#[async_trait]
impl AiProvider for OpenCodeProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: "opencode:cli",
            metadata: ProviderMetadata {
                kind: ProviderKind::OpenCode,
                display_name: "OpenCode".into(),
                brand_name: "OpenCode".into(),
                icon_asset: "src/icons/provider-opencode.svg".into(),
                dashboard_url: "https://opencode.ai".into(),
                account_hint: "OpenCode account".into(),
                source_label: "opencode api".into(),
            },
        }
    }

    async fn check_availability(&self) -> Result<()> {
        if cli::command_exists(OPENCODE_CLI) {
            Ok(())
        } else {
            Err(ProviderError::cli_not_found(OPENCODE_CLI).into())
        }
    }

    async fn refresh(&self) -> Result<RefreshData> {
        Err(ProviderError::unavailable(
            "OpenCode requires an active session, usage monitoring not supported yet",
        )
        .into())
    }
}
