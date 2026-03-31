use super::AiProvider;
use crate::models::{ProviderKind, ProviderMetadata, ProviderStatus};
use anyhow::{bail, Result};
use log::{debug, info, warn};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Provider 聚合管理器，持有各类实际 Provider 实现
pub struct ProviderManager {
    pub(crate) providers: Vec<Arc<dyn AiProvider>>,
    providers_by_kind: HashMap<ProviderKind, Arc<dyn AiProvider>>,
    metadata_by_kind: HashMap<ProviderKind, ProviderMetadata>,
    provider_ids: HashSet<&'static str>,
}

impl Default for ProviderManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderManager {
    pub fn new() -> Self {
        let mut manager = Self {
            providers: Vec::new(),
            providers_by_kind: HashMap::new(),
            metadata_by_kind: HashMap::new(),
            provider_ids: HashSet::new(),
        };

        // 注册所有已实现的 Provider
        super::register_all(&mut manager);

        manager
    }

    pub fn register(&mut self, provider: Arc<dyn AiProvider>) {
        let provider_id = provider.id();
        let kind = provider.kind();
        if self.provider_ids.contains(provider_id) {
            warn!(
                target: "providers",
                "provider id already registered, skipping duplicate: {}",
                provider_id
            );
            return;
        }
        if self.providers_by_kind.contains_key(&kind) {
            warn!(
                target: "providers",
                "provider kind already registered, skipping duplicate: {:?}",
                kind
            );
            return;
        }
        info!(target: "providers", "registering provider: {} ({:?})", provider_id, kind);
        let metadata = provider.metadata();
        debug_assert_eq!(metadata.kind, kind);

        self.provider_ids.insert(provider_id);
        self.metadata_by_kind.insert(kind, metadata);
        self.providers_by_kind.insert(kind, provider.clone());
        self.providers.push(provider);
    }

    fn provider_for_kind(&self, kind: ProviderKind) -> Option<&(dyn AiProvider + '_)> {
        self.providers_by_kind.get(&kind).map(Arc::as_ref)
    }

    pub fn metadata_for(&self, kind: ProviderKind) -> ProviderMetadata {
        self.metadata_by_kind
            .get(&kind)
            .cloned()
            .unwrap_or_else(|| ProviderMetadata::fallback(kind))
    }

    /// 为 App 提供所有的预设状态，未支持的 Provider 默认为 Disconnected
    pub fn initial_statuses(&self) -> Vec<ProviderStatus> {
        ProviderKind::all()
            .iter()
            .map(|kind| ProviderStatus::new(self.metadata_for(*kind)))
            .collect()
    }

    /// 刷新指定的 Provider
    pub async fn refresh_provider(&self, kind: ProviderKind) -> Result<crate::models::RefreshData> {
        debug!(target: "providers", "manager: refreshing provider {:?}", kind);
        if let Some(provider) = self.provider_for_kind(kind) {
            if provider.is_available().await {
                return provider.refresh().await;
            }

            let metadata = self.metadata_for(kind);
            warn!(
                target: "providers",
                "provider {} is unavailable",
                metadata.display_name
            );
            bail!(
                "Provider {} is currently unavailable in this environment.",
                metadata.display_name
            );
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

    #[test]
    fn test_no_duplicate_provider_kinds() {
        let manager = ProviderManager::new();
        let mut kinds = std::collections::HashSet::new();
        for provider in &manager.providers {
            let kind = provider.kind();
            assert!(kinds.insert(kind), "Duplicate provider kind: {:?}", kind);
        }
        assert_eq!(manager.providers.len(), manager.providers_by_kind.len());
    }
}
