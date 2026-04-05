use super::AiProvider;
use crate::models::{ProviderId, ProviderKind, ProviderMetadata, ProviderStatus};
use anyhow::Result;
use log::{debug, info, warn};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Provider 聚合管理器，持有各类实际 Provider 实现
pub struct ProviderManager {
    pub(crate) providers: Vec<Arc<dyn AiProvider>>,
    providers_by_kind: HashMap<ProviderKind, Arc<dyn AiProvider>>,
    metadata_by_kind: HashMap<ProviderKind, ProviderMetadata>,
    provider_ids: HashSet<Cow<'static, str>>,
    /// 自定义 Provider（按 ID 索引）
    custom_providers_by_id: HashMap<String, Arc<dyn AiProvider>>,
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
            custom_providers_by_id: HashMap::new(),
        };

        // 注册所有内置 Provider
        super::register_all(&mut manager);

        // 加载自定义 Provider
        manager.load_custom_providers();

        manager
    }

    pub fn register(&mut self, provider: Arc<dyn AiProvider>) {
        let descriptor = provider.descriptor();
        let provider_id = descriptor.id.clone();
        let kind = descriptor.kind();
        if self.provider_ids.contains(&provider_id) {
            warn!(
                target: "providers",
                "provider id already registered, skipping duplicate: {}",
                provider_id
            );
            return;
        }
        // 内置 Provider 按 kind 去重；自定义 Provider 按 id 去重
        if kind != ProviderKind::Custom && self.providers_by_kind.contains_key(&kind) {
            warn!(
                target: "providers",
                "provider kind already registered, skipping duplicate: {:?}",
                kind
            );
            return;
        }
        info!(target: "providers", "registering provider: {} ({:?})", provider_id, kind);
        let metadata = descriptor.metadata;
        debug_assert_eq!(metadata.kind, kind);

        self.provider_ids.insert(provider_id.clone());
        if kind == ProviderKind::Custom {
            // Custom provider 按 ID 索引，不写入 kind 维度的映射（避免互相覆写）
            self.custom_providers_by_id
                .insert(provider_id.to_string(), provider.clone());
        } else {
            self.metadata_by_kind.insert(kind, metadata);
            self.providers_by_kind.insert(kind, provider.clone());
        }
        self.providers.push(provider);
    }

    fn load_custom_providers(&mut self) {
        let custom = super::custom::load_custom_providers();
        for provider in custom {
            self.register(Arc::new(provider));
        }
    }

    fn provider_for_kind(&self, kind: ProviderKind) -> Option<&(dyn AiProvider + '_)> {
        self.providers_by_kind.get(&kind).map(Arc::as_ref)
    }

    /// 按 ID 查找自定义 Provider
    pub fn custom_provider_by_id(&self, id: &str) -> Option<&(dyn AiProvider + '_)> {
        self.custom_providers_by_id.get(id).map(Arc::as_ref)
    }

    /// 获取所有自定义 Provider 的 ID 列表
    #[allow(dead_code)]
    pub fn custom_provider_ids(&self) -> Vec<String> {
        self.custom_providers_by_id.keys().cloned().collect()
    }

    pub fn metadata_for(&self, kind: ProviderKind) -> ProviderMetadata {
        self.metadata_by_kind
            .get(&kind)
            .cloned()
            .unwrap_or_else(|| ProviderMetadata::fallback(kind))
    }

    /// 为 App 提供所有的预设状态（内置 + 自定义），默认为 Disconnected
    pub fn initial_statuses(&self) -> Vec<ProviderStatus> {
        let mut statuses: Vec<ProviderStatus> = ProviderKind::all()
            .iter()
            .map(|kind| ProviderStatus::new(self.metadata_for(*kind)))
            .collect();

        // 追加自定义 Provider 状态
        for (id, provider) in &self.custom_providers_by_id {
            let descriptor = provider.descriptor();
            let provider_id = ProviderId::Custom(id.clone());
            statuses.push(ProviderStatus::new_custom(provider_id, descriptor.metadata));
        }

        statuses
    }

    /// 刷新指定的 Provider
    pub async fn refresh_provider(&self, kind: ProviderKind) -> Result<crate::models::RefreshData> {
        debug!(target: "providers", "manager: refreshing provider {:?}", kind);
        if let Some(provider) = self.provider_for_kind(kind) {
            if let Err(err) = provider.check_availability().await {
                let classified = super::ProviderError::classify(&err);
                let metadata = self.metadata_for(kind);
                warn!(
                    target: "providers",
                    "provider {} is unavailable: {}",
                    metadata.display_name,
                    classified
                );
                return Err(classified.into());
            }
            return provider.refresh().await;
        }
        Err(super::ProviderError::unavailable(&format!(
            "No implementation registered for provider {:?}",
            kind
        ))
        .into())
    }

    /// 刷新指定 ID 的自定义 Provider
    pub async fn refresh_custom_provider(&self, id: &str) -> Result<crate::models::RefreshData> {
        debug!(target: "providers", "manager: refreshing custom provider {}", id);
        if let Some(provider) = self.custom_provider_by_id(id) {
            if let Err(err) = provider.check_availability().await {
                let classified = super::ProviderError::classify(&err);
                warn!(
                    target: "providers",
                    "custom provider {} is unavailable: {}",
                    id,
                    classified
                );
                return Err(classified.into());
            }
            return provider.refresh().await;
        }
        Err(super::ProviderError::unavailable(&format!(
            "No custom provider registered with id: {}",
            id
        ))
        .into())
    }

    /// 统一的刷新方法：根据 ProviderId 路由到对应的 Provider
    pub async fn refresh_by_id(&self, id: &ProviderId) -> Result<crate::models::RefreshData> {
        match id {
            ProviderId::BuiltIn(kind) => self.refresh_provider(*kind).await,
            ProviderId::Custom(custom_id) => self.refresh_custom_provider(custom_id).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_provider_kinds_have_implementation() {
        let manager = ProviderManager::new();
        for kind in ProviderKind::all() {
            let found = manager
                .providers
                .iter()
                .any(|p| p.descriptor().kind() == *kind);
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
            let id = p.descriptor().id;
            assert!(ids.insert(id.clone()), "Duplicate provider id: {}", id);
        }
    }

    #[test]
    fn test_no_duplicate_builtin_provider_kinds() {
        let manager = ProviderManager::new();
        let mut kinds = std::collections::HashSet::new();
        for provider in &manager.providers {
            let kind = provider.descriptor().kind();
            if kind != ProviderKind::Custom {
                assert!(kinds.insert(kind), "Duplicate provider kind: {:?}", kind);
            }
        }
    }
}
