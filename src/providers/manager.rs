use super::AiProvider;
use crate::models::{ConnectionStatus, ProviderKind, ProviderMetadata, ProviderStatus};
use anyhow::{bail, Result};
use std::sync::Arc;

/// Provider 聚合管理器，持有各类实际 Provider 实现
pub struct ProviderManager {
    pub(crate) providers: Vec<Arc<dyn AiProvider>>,
}

impl ProviderManager {
    pub fn new() -> Self {
        let mut manager = Self {
            providers: Vec::new(),
        };

        // 注册所有已实现的 Provider
        super::register_all(&mut manager);

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
            let metadata = if let Some(p) = self.providers.iter().find(|p| p.kind() == *kind) {
                p.metadata()
            } else {
                ProviderMetadata {
                    kind: *kind,
                    display_name: format!("{:?}", kind),
                    brand_name: format!("{:?}", kind),
                    source_label: "unknown".to_string(),
                    account_hint: "account".to_string(),
                    icon_asset: "src/icons/provider-unknown.svg".to_string(),
                    dashboard_url: "".to_string(),
                }
            };

            statuses.push(ProviderStatus {
                kind: *kind,
                metadata,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_provider_kinds_have_implementation() {
        let manager = ProviderManager::new();
        for kind in ProviderKind::all() {
            let found = manager.providers.iter().any(|p| p.kind() == *kind);
            assert!(
                found,
                "ProviderKind::{:?} is defined in models but NOT registered in ProviderManager.
                Please add it to register_providers! macro in src/providers/mod.rs",
                kind
            );
        }
    }

    #[test]
    fn test_no_duplicate_provider_ids() {
        let manager = ProviderManager::new();
        let mut ids = std::collections::HashSet::new();
        for p in &manager.providers {
            let id = p.id();
            assert!(ids.insert(id), "Duplicate provider id: {}", id);
        }
    }
}
