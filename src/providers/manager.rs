use super::AiProvider;
use crate::models::{
    AppSettings, ProviderId, ProviderKind, ProviderMetadata, ProviderStatus, TokenInputCapability,
    TokenInputState,
};
use anyhow::Result;
use log::{debug, info, warn};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Provider 聚合管理器，持有各类实际 Provider 实现
pub struct ProviderManager {
    providers: Vec<Arc<dyn AiProvider>>,
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
            .map(|kind| {
                let mut status =
                    ProviderStatus::new(ProviderId::BuiltIn(*kind), self.metadata_for(*kind));
                if let Some(provider) = self.provider_for_kind(*kind) {
                    status.settings_capability = provider.settings_capability();
                }
                status
            })
            .collect();

        // 追加自定义 Provider 状态
        for (id, provider) in &self.custom_providers_by_id {
            let descriptor = provider.descriptor();
            let provider_id = ProviderId::Custom(id.clone());
            let mut status = ProviderStatus::new(provider_id, descriptor.metadata);
            status.settings_capability = provider.settings_capability();
            statuses.push(status);
        }

        statuses
    }

    /// 根据 ProviderId + capability 解析 TokenInput 面板的运行时展示状态。
    ///
    /// 优先走 provider 自定义解析；若 provider 未注册或未覆写，则回落到通用 credential 存储。
    pub fn resolve_token_input_state(
        &self,
        id: &ProviderId,
        capability: TokenInputCapability,
        settings: &AppSettings,
    ) -> TokenInputState {
        let provider = match id {
            ProviderId::BuiltIn(kind) => self.provider_for_kind(*kind),
            ProviderId::Custom(custom_id) => self.custom_provider_by_id(custom_id),
        };
        provider
            .and_then(|provider| provider.resolve_token_input_state(settings))
            .unwrap_or_else(|| {
                super::default_token_input_state(settings, capability.credential_key)
            })
    }

    /// 统一的刷新方法：根据 ProviderId 路由到对应的 Provider
    pub async fn refresh_by_id(&self, id: &ProviderId) -> Result<crate::models::RefreshData> {
        debug!(target: "providers", "manager: refreshing provider {}", id);
        let provider = match id {
            ProviderId::BuiltIn(kind) => self.provider_for_kind(*kind),
            ProviderId::Custom(custom_id) => self.custom_provider_by_id(custom_id),
        };
        match provider {
            Some(p) => {
                let display_label = Self::display_label_for(id, p);
                Self::execute_refresh(p, &display_label).await
            }
            None => Err(super::ProviderError::unavailable(&format!(
                "No provider registered for {}",
                id
            ))
            .into()),
        }
    }

    fn display_label_for(id: &ProviderId, provider: &dyn AiProvider) -> String {
        match id {
            ProviderId::BuiltIn(_) => provider.descriptor().metadata.display_name,
            ProviderId::Custom(_) => id.to_string(),
        }
    }

    /// 通用刷新执行：check_availability → refresh
    async fn execute_refresh(
        provider: &dyn AiProvider,
        display_label: &str,
    ) -> Result<crate::models::RefreshData> {
        if let Err(err) = provider.check_availability().await {
            let classified = super::ProviderError::classify(&err);
            warn!(
                target: "providers",
                "provider {} is unavailable: {}",
                display_label,
                classified
            );
            return Err(classified.into());
        }
        provider.refresh().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        AppSettings, SettingsCapability, TokenEditMode, TokenInputCapability, TokenInputState,
    };
    use anyhow::Result;
    use async_trait::async_trait;
    use std::borrow::Cow;
    use std::collections::{HashMap, HashSet};

    struct TestProvider {
        descriptor: crate::models::ProviderDescriptor,
    }

    struct DefaultTokenProvider {
        descriptor: crate::models::ProviderDescriptor,
    }

    #[async_trait]
    impl AiProvider for TestProvider {
        fn descriptor(&self) -> crate::models::ProviderDescriptor {
            self.descriptor.clone()
        }

        fn settings_capability(&self) -> SettingsCapability {
            SettingsCapability::TokenInput(TokenInputCapability {
                credential_key: "test_token",
                placeholder_i18n_key: "copilot.token_placeholder",
                help_tip_i18n_key: "copilot.token_sources_tip",
                title_i18n_key: "copilot.github_login",
                description_i18n_key: "copilot.requires_auth",
                create_url: "https://example.com/token",
            })
        }

        fn resolve_token_input_state(&self, _settings: &AppSettings) -> Option<TokenInputState> {
            Some(TokenInputState {
                has_token: true,
                masked: Some("test•••oken".to_string()),
                source_i18n_key: Some("copilot.source.env_var"),
                edit_mode: TokenEditMode::SetNew,
            })
        }

        async fn refresh(&self) -> Result<crate::models::RefreshData> {
            Ok(crate::models::RefreshData::quotas_only(Vec::new()))
        }
    }

    #[async_trait]
    impl AiProvider for DefaultTokenProvider {
        fn descriptor(&self) -> crate::models::ProviderDescriptor {
            self.descriptor.clone()
        }

        fn settings_capability(&self) -> SettingsCapability {
            SettingsCapability::TokenInput(TokenInputCapability {
                credential_key: "test_token",
                placeholder_i18n_key: "copilot.token_placeholder",
                help_tip_i18n_key: "copilot.token_sources_tip",
                title_i18n_key: "copilot.github_login",
                description_i18n_key: "copilot.requires_auth",
                create_url: "https://example.com/token",
            })
        }

        async fn refresh(&self) -> Result<crate::models::RefreshData> {
            Ok(crate::models::RefreshData::quotas_only(Vec::new()))
        }
    }

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

    #[test]
    fn test_display_label_for_builtin_uses_human_readable_name() {
        let provider = TestProvider {
            descriptor: crate::models::ProviderDescriptor {
                id: Cow::Borrowed("amp"),
                metadata: ProviderMetadata {
                    kind: ProviderKind::Amp,
                    display_name: "Amp".to_string(),
                    brand_name: "Amp".to_string(),
                    icon_asset: String::new(),
                    dashboard_url: String::new(),
                    account_hint: String::new(),
                    source_label: String::new(),
                },
            },
        };

        let label =
            ProviderManager::display_label_for(&ProviderId::BuiltIn(ProviderKind::Amp), &provider);
        assert_eq!(label, "Amp");
    }

    #[test]
    fn test_display_label_for_custom_keeps_stable_id() {
        let provider = TestProvider {
            descriptor: crate::models::ProviderDescriptor {
                id: Cow::Owned("demo:custom".to_string()),
                metadata: ProviderMetadata {
                    kind: ProviderKind::Custom,
                    display_name: "Demo Provider".to_string(),
                    brand_name: "Demo Provider".to_string(),
                    icon_asset: String::new(),
                    dashboard_url: String::new(),
                    account_hint: String::new(),
                    source_label: String::new(),
                },
            },
        };

        let label = ProviderManager::display_label_for(
            &ProviderId::Custom("demo:custom".to_string()),
            &provider,
        );
        assert_eq!(label, "demo:custom");
    }

    #[test]
    fn test_resolve_token_input_state_routes_to_provider_override() {
        let mut manager = ProviderManager {
            providers: Vec::new(),
            providers_by_kind: HashMap::new(),
            metadata_by_kind: HashMap::new(),
            provider_ids: HashSet::new(),
            custom_providers_by_id: HashMap::new(),
        };
        manager.register(Arc::new(TestProvider {
            descriptor: crate::models::ProviderDescriptor {
                id: Cow::Borrowed("amp"),
                metadata: ProviderMetadata {
                    kind: ProviderKind::Amp,
                    display_name: "Amp".to_string(),
                    brand_name: "Amp".to_string(),
                    icon_asset: String::new(),
                    dashboard_url: String::new(),
                    account_hint: String::new(),
                    source_label: String::new(),
                },
            },
        }));

        let state = manager.resolve_token_input_state(
            &ProviderId::BuiltIn(ProviderKind::Amp),
            TokenInputCapability {
                credential_key: "test_token",
                placeholder_i18n_key: "copilot.token_placeholder",
                help_tip_i18n_key: "copilot.token_sources_tip",
                title_i18n_key: "copilot.github_login",
                description_i18n_key: "copilot.requires_auth",
                create_url: "https://example.com/token",
            },
            &AppSettings::default(),
        );

        assert!(state.has_token);
        assert_eq!(state.edit_mode, TokenEditMode::SetNew);
        assert_eq!(state.source_i18n_key, Some("copilot.source.env_var"));
    }

    #[test]
    fn test_resolve_token_input_state_falls_back_to_default_credential_store() {
        let mut manager = ProviderManager {
            providers: Vec::new(),
            providers_by_kind: HashMap::new(),
            metadata_by_kind: HashMap::new(),
            provider_ids: HashSet::new(),
            custom_providers_by_id: HashMap::new(),
        };
        manager.register(Arc::new(DefaultTokenProvider {
            descriptor: crate::models::ProviderDescriptor {
                id: Cow::Borrowed("amp"),
                metadata: ProviderMetadata {
                    kind: ProviderKind::Amp,
                    display_name: "Amp".to_string(),
                    brand_name: "Amp".to_string(),
                    icon_asset: String::new(),
                    dashboard_url: String::new(),
                    account_hint: String::new(),
                    source_label: String::new(),
                },
            },
        }));

        let mut settings = AppSettings::default();
        settings
            .provider
            .credentials
            .set_credential("test_token", "abcd1234wxyz".to_string());

        let state = manager.resolve_token_input_state(
            &ProviderId::BuiltIn(ProviderKind::Amp),
            TokenInputCapability {
                credential_key: "test_token",
                placeholder_i18n_key: "copilot.token_placeholder",
                help_tip_i18n_key: "copilot.token_sources_tip",
                title_i18n_key: "copilot.github_login",
                description_i18n_key: "copilot.requires_auth",
                create_url: "https://example.com/token",
            },
            &settings,
        );

        assert!(state.has_token);
        assert_eq!(state.edit_mode, TokenEditMode::EditStored);
        assert_eq!(state.source_i18n_key, None);
        assert_eq!(state.masked.as_deref(), Some("abcd•••wxyz"));
    }
}
