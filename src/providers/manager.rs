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
        manager.register(Arc::new(super::gemini::GeminiProvider::new()));
        manager.register(Arc::new(super::amp::AmpProvider::new()));
        manager.register(Arc::new(super::copilot::CopilotProvider::new()));
        manager.register(Arc::new(super::codex::CodexProvider::new()));
        manager.register(Arc::new(super::kimi::KimiProvider::new()));
        manager.register(Arc::new(super::cursor::CursorProvider::new()));
        manager.register(Arc::new(super::opencode::OpenCodeProvider::new()));
        manager.register(Arc::new(super::minimax::MiniMaxProvider::new()));
        manager.register(Arc::new(super::vertex_ai::VertexAiProvider::new()));
        manager.register(Arc::new(super::kilo::KiloProvider::new()));
        manager.register(Arc::new(super::kiro::KiroProvider::new()));

        manager
    }

    pub fn register(&mut self, provider: Arc<dyn AiProvider>) {
        let provider_id = provider.id();
        if self
            .providers
            .iter()
            .any(|existing| existing.id() == provider_id)
        {
            return;
        }
        self.providers.push(provider);
    }

    /// 为 App 提供所有的预设状态，未支持的 Provider 默认为 Disconnected
    pub fn initial_statuses(&self) -> Vec<ProviderStatus> {
        let mut statuses = Vec::new();
        for kind in ProviderKind::all() {
            let (display_name, brand_name, source_label, account_hint, icon_asset, dashboard_url) =
                if let Some(p) = self.providers.iter().find(|p| p.kind() == *kind) {
                    let m = p.metadata();
                    (
                        m.display_name.to_string(),
                        m.brand_name.to_string(),
                        m.source_label.to_string(),
                        m.account_hint.to_string(),
                        m.icon_asset.to_string(),
                        m.dashboard_url.to_string(),
                    )
                } else {
                    (
                        format!("{:?}", kind),
                        format!("{:?}", kind),
                        "unknown".to_string(),
                        "account".to_string(),
                        "src/icons/provider-unknown.svg".to_string(),
                        "".to_string(),
                    )
                };

            statuses.push(ProviderStatus {
                kind: *kind,
                display_name,
                brand_name,
                source_label,
                account_hint,
                icon_asset,
                dashboard_url,
                enabled: true,
                connection: ConnectionStatus::Disconnected,
                quotas: vec![],
                account_email: None,
                is_paid: false,
                account_tier: None,
                last_updated_at: None,
                error_message: None,
                last_refreshed_instant: None,
            });
        }
        statuses
    }

    /// 检查指定 Provider 是否可用
    pub async fn is_provider_available(&self, kind: ProviderKind) -> bool {
        for p in &self.providers {
            if p.kind() == kind {
                return p.is_available().await;
            }
        }
        false
    }

    /// 刷新指定的 Provider
    pub async fn refresh_provider(
        &self,
        kind: ProviderKind,
    ) -> Result<Vec<crate::models::QuotaInfo>> {
        for p in &self.providers {
            if p.kind() == kind {
                if p.is_available().await {
                    return p.refresh().await;
                } else {
                    let meta = p.metadata();
                    bail!(
                        "Provider {} is currently unavailable in this environment.",
                        meta.display_name
                    );
                }
            }
        }
        bail!("No implementation registered for provider {:?}", kind)
    }
}
