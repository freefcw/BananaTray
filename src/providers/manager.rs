use super::AiProvider;
use crate::models::{ConnectionStatus, ProviderKind, ProviderStatus};
use anyhow::{bail, Result};
use std::sync::Arc;

/// Provider 聚合管理器，持有各类实际 Provider 实现
pub struct ProviderManager {
    providers: Vec<Arc<dyn AiProvider>>,
}

impl ProviderManager {
    pub fn new() -> Self {
        let mut manager = Self {
            providers: Vec::new(),
        };

        // 注册已有的真实 Provider (比如 claude)
        manager.register(Arc::new(super::claude::ClaudeProvider::new()));
        manager.register(Arc::new(super::amp::AmpProvider::new()));
        manager.register(Arc::new(super::copilot::CopilotProvider::new()));

        manager
    }

    pub fn register(&mut self, provider: Arc<dyn AiProvider>) {
        self.providers.push(provider);
    }

    /// 为 App 提供所有的预设状态，未支持的 Provider 默认为 Disconnected
    pub fn initial_statuses(&self) -> Vec<ProviderStatus> {
        let mut statuses = Vec::new();
        for kind in ProviderKind::all() {
            statuses.push(ProviderStatus {
                kind: *kind,
                enabled: true,
                connection: ConnectionStatus::Disconnected,
                quotas: Vec::new(),
            });
        }
        statuses
    }

    /// 刷新指定的 Provider
    pub async fn refresh_provider(&self, kind: ProviderKind) -> Result<Vec<crate::models::QuotaInfo>> {
        for p in &self.providers {
            if p.kind() == kind {
                if p.is_available().await {
                    return p.refresh().await;
                } else {
                    bail!("Provider {} is currently unavailable in this environment.", kind.display_name());
                }
            }
        }
        bail!("No implementation registered for provider {:?}", kind)
    }
}
