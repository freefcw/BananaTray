use super::AiProvider;
use crate::models::{ProviderKind, QuotaInfo};
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
    fn id(&self) -> &'static str {
        "opencode:cli"
    }

    fn kind(&self) -> ProviderKind {
        ProviderKind::OpenCode
    }

    async fn is_available(&self) -> bool {
        Command::new("opencode").arg("--version").output().is_ok()
    }

    async fn refresh(&self) -> Result<Vec<QuotaInfo>> {
        bail!("OpenCode usage monitoring requires a running opencode session. No public API available yet.")
    }
}
