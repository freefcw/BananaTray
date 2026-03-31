use super::{AiProvider, ProviderError};
use crate::models::{ProviderKind, ProviderMetadata, QuotaInfo};
use anyhow::Result;
use async_trait::async_trait;
use std::process::Command;

super::define_unit_provider!(OpenCodeProvider);

#[async_trait]
impl AiProvider for OpenCodeProvider {
    fn metadata(&self) -> ProviderMetadata {
        ProviderMetadata {
            kind: ProviderKind::OpenCode,
            display_name: "OpenCode".into(),
            brand_name: "OpenCode".into(),
            icon_asset: "src/icons/provider-opencode.svg".into(),
            dashboard_url: "https://opencode.ai".into(),
            account_hint: "OpenCode account".into(),
            source_label: "opencode api".into(),
        }
    }

    fn id(&self) -> &'static str {
        "opencode:cli"
    }

    async fn is_available(&self) -> bool {
        Command::new("opencode").arg("--version").output().is_ok()
    }

    async fn refresh_quotas(&self) -> Result<Vec<QuotaInfo>> {
        Err(ProviderError::unavailable("OpenCode 需要运行中的会话，暂不支持用量监控").into())
    }
}
