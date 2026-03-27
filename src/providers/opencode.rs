use super::AiProvider;
use crate::models::{ProviderKind, ProviderMetadata, QuotaInfo};
use anyhow::{bail, Result};
use async_trait::async_trait;
use std::process::Command;

pub struct OpenCodeProvider {}

impl OpenCodeProvider {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl AiProvider for OpenCodeProvider {
    fn metadata(&self) -> ProviderMetadata {
        ProviderMetadata {
            kind: ProviderKind::OpenCode,
            display_name: "OpenCode",
            brand_name: "OpenCode",
            icon_asset: "src/icons/provider-opencode.svg",
            dashboard_url: "https://opencode.ai",
            account_hint: "OpenCode account",
            source_label: "opencode api",
        }
    }

    fn id(&self) -> &'static str {
        "opencode:cli"
    }

    async fn is_available(&self) -> bool {
        Command::new("opencode").arg("--version").output().is_ok()
    }

    async fn refresh(&self) -> Result<Vec<QuotaInfo>> {
        bail!("OpenCode usage monitoring requires a running opencode session. No public API available yet.")
    }
}
