use super::{ProviderEntry, ProviderError, ProviderExecutionContext, ProviderResult};
use crate::models::{
    AppSettings, ProviderId, ProviderKind, ProviderMetadata, ProviderSettings, ProviderStatus,
    TokenInputCapability, TokenInputState,
};
use log::{debug, info, warn};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// ProviderManager 的共享快照句柄。
///
/// 前台 UI 和后台 refresh 协调器通过同一个句柄读取当前快照；
/// 热重载自定义 provider 时，仅替换内部 `Arc<ProviderManager>`，
/// 避免前后台各自持有不同 manager 实例。
/// `Default` 只构造内置、纯内存 registry；磁盘加载必须由组合根显式完成。
#[derive(Clone)]
pub struct ProviderManagerHandle {
    inner: Arc<RwLock<Arc<ProviderManager>>>,
}

impl ProviderManagerHandle {
    /// 使用给定 manager 创建共享句柄。
    pub fn new(manager: ProviderManager) -> Self {
        Self::from_arc(Arc::new(manager))
    }

    /// 使用已有的 manager 快照创建共享句柄。
    pub fn from_arc(manager: Arc<ProviderManager>) -> Self {
        Self {
            inner: Arc::new(RwLock::new(manager)),
        }
    }

    /// 读取当前 manager 快照，供单次操作使用。
    pub fn snapshot(&self) -> Arc<ProviderManager> {
        self.inner
            .read()
            .expect("provider manager handle read lock poisoned")
            .clone()
    }

    /// 原子替换当前 manager 快照。
    pub fn replace(&self, manager: Arc<ProviderManager>) {
        *self
            .inner
            .write()
            .expect("provider manager handle write lock poisoned") = manager;
    }
}

impl Default for ProviderManagerHandle {
    fn default() -> Self {
        Self::new(ProviderManager::new())
    }
}

/// Provider 聚合管理器，持有各类实际 Provider 实现
///
/// 仅维护两个索引，与 `ProviderId` 的两个变体一一对应：
/// - 内置 Provider 按 `ProviderKind` 查找
/// - 自定义 Provider 按字符串 ID 查找
pub struct ProviderManager {
    providers_by_kind: HashMap<ProviderKind, Arc<dyn ProviderEntry>>,
    custom_providers_by_id: HashMap<String, Arc<dyn ProviderEntry>>,
}

impl Default for ProviderManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderManager {
    fn empty() -> Self {
        Self {
            providers_by_kind: HashMap::new(),
            custom_providers_by_id: HashMap::new(),
        }
    }

    /// 构造只包含内置 Provider 的纯注册表，不执行文件系统 I/O。
    pub fn new() -> Self {
        Self::with_custom_providers(std::iter::empty::<Arc<dyn ProviderEntry>>())
    }

    /// 从默认配置目录加载 custom Provider，并与内置 Provider 合并。
    pub fn load_default() -> Self {
        let custom_providers = super::custom::load_custom_providers()
            .into_iter()
            .map(|provider| Arc::new(provider) as Arc<dyn ProviderEntry>);
        Self::with_custom_providers(custom_providers)
    }

    pub(crate) fn with_custom_providers<I>(custom_providers: I) -> Self
    where
        I: IntoIterator<Item = Arc<dyn ProviderEntry>>,
    {
        let mut manager = Self::empty();
        super::register_all(&mut manager);
        manager.register_custom_providers(custom_providers);
        manager
    }

    pub fn register(&mut self, provider: Arc<dyn ProviderEntry>) {
        let descriptor = provider.descriptor();
        let provider_id = descriptor.id.clone();
        let kind = descriptor.kind();

        if self.has_registered_id(provider_id.as_ref()) {
            warn!(
                target: "providers",
                "provider id already registered, skipping duplicate: {}",
                provider_id
            );
            return;
        }
        if kind == ProviderKind::Custom && ProviderKind::from_id_key(provider_id.as_ref()).is_some()
        {
            warn!(
                target: "providers",
                "custom provider id is reserved by a built-in provider, skipping: {}",
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
        debug_assert_eq!(descriptor.metadata.kind, kind);

        if kind == ProviderKind::Custom {
            self.custom_providers_by_id
                .insert(provider_id.to_string(), provider);
        } else {
            self.providers_by_kind.insert(kind, provider);
        }
    }

    /// 注册 manifest 声明的内置 Provider。
    ///
    /// manifest kind 与实现 descriptor kind 都是编译期维护的程序不变量；
    /// 一旦错配立即 panic，避免实现被静默挂到另一个持久化身份下。
    pub(crate) fn register_builtin(
        &mut self,
        manifest_kind: ProviderKind,
        provider: Arc<dyn ProviderEntry>,
    ) {
        let descriptor = provider.descriptor();
        assert_ne!(
            manifest_kind,
            ProviderKind::Custom,
            "built-in provider manifest cannot declare ProviderKind::Custom"
        );
        assert_eq!(
            descriptor.kind(),
            manifest_kind,
            "built-in provider manifest kind mismatch for descriptor {}",
            descriptor.id
        );
        self.register(provider);
    }

    /// 检查某个 provider ID 是否已被注册
    fn has_registered_id(&self, id: &str) -> bool {
        self.custom_providers_by_id.contains_key(id)
            || self
                .providers_by_kind
                .values()
                .any(|p| p.descriptor().id.as_ref() == id)
    }

    fn register_custom_providers<I>(&mut self, custom_providers: I)
    where
        I: IntoIterator<Item = Arc<dyn ProviderEntry>>,
    {
        for provider in custom_providers {
            self.register(provider);
        }
    }

    fn provider_for_kind(&self, kind: ProviderKind) -> Option<&(dyn ProviderEntry + '_)> {
        self.providers_by_kind.get(&kind).map(Arc::as_ref)
    }

    /// 按 ID 查找自定义 Provider
    pub fn custom_provider_by_id(&self, id: &str) -> Option<&(dyn ProviderEntry + '_)> {
        self.custom_providers_by_id.get(id).map(Arc::as_ref)
    }

    /// 获取所有自定义 Provider 的 ID 列表
    #[allow(dead_code)]
    pub fn custom_provider_ids(&self) -> Vec<String> {
        self.custom_providers_by_id.keys().cloned().collect()
    }

    pub fn metadata_for(&self, kind: ProviderKind) -> ProviderMetadata {
        self.provider_for_kind(kind)
            .map(|p| p.descriptor().metadata)
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
                    status.provider_capability = provider.provider_capability();
                }
                status
            })
            .collect();

        // 追加自定义 Provider 状态；输出前按 ID 排序，避免 HashMap 迭代顺序泄漏到首次 sidebar/order。
        let mut custom_entries: Vec<_> = self.custom_providers_by_id.iter().collect();
        custom_entries.sort_by_key(|(id, _)| id.as_str());
        for (id, provider) in custom_entries {
            let descriptor = provider.descriptor();
            let provider_id = ProviderId::Custom(id.clone());
            let mut status = ProviderStatus::new(provider_id, descriptor.metadata);
            status.settings_capability = provider.settings_capability();
            status.provider_capability = provider.provider_capability();
            statuses.push(status);
        }

        statuses
    }

    /// 按 ProviderId 统一查找 Provider
    fn provider_for_id(&self, id: &ProviderId) -> Option<&(dyn ProviderEntry + '_)> {
        match id {
            ProviderId::BuiltIn(kind) => self.provider_for_kind(*kind),
            ProviderId::Custom(custom_id) => self.custom_provider_by_id(custom_id),
        }
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
        self.provider_for_id(id)
            .and_then(|provider| provider.resolve_token_input_state(settings))
            .unwrap_or_else(|| {
                super::default_token_input_state(settings, capability.credential_key)
            })
    }

    pub async fn refresh_by_id(
        &self,
        id: &ProviderId,
        provider_credentials: &ProviderSettings,
    ) -> ProviderResult<crate::models::RefreshData> {
        debug!(target: "providers", "manager: refreshing provider {}", id);
        let ctx = ProviderExecutionContext {
            provider_credentials,
        };
        match self.provider_for_id(id) {
            Some(p) => {
                if !p.provider_capability().supports_refresh() {
                    return Err(ProviderError::NoData);
                }
                let display_label = Self::display_label_for(id, p);
                Self::execute_refresh(p, &display_label, &ctx).await
            }
            None => Err(ProviderError::unavailable(&format!(
                "No provider registered for {}",
                id
            ))),
        }
    }

    fn display_label_for(id: &ProviderId, provider: &dyn ProviderEntry) -> String {
        match id {
            ProviderId::BuiltIn(_) => provider.descriptor().metadata.display_name,
            ProviderId::Custom(_) => id.to_string(),
        }
    }

    /// 通用刷新执行：check_availability → refresh
    async fn execute_refresh(
        provider: &dyn ProviderEntry,
        display_label: &str,
        ctx: &ProviderExecutionContext<'_>,
    ) -> ProviderResult<crate::models::RefreshData> {
        if let Err(err) = provider.check_availability(ctx).await {
            warn!(
                target: "providers",
                "provider {} is unavailable: {}",
                display_label,
                err
            );
            return Err(err);
        }
        provider.refresh(ctx).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        AppSettings, ProviderSettings, SettingsCapability, TokenEditMode, TokenInputCapability,
        TokenInputState,
    };
    use crate::providers::{AiProvider, ProviderCapabilities};
    use async_trait::async_trait;
    use std::borrow::Cow;
    use std::ffi::{OsStr, OsString};

    struct ScopedEnvOverride {
        name: &'static str,
        previous: Option<OsString>,
    }

    impl ScopedEnvOverride {
        fn set(name: &'static str, value: impl AsRef<OsStr>) -> Self {
            let previous = std::env::var_os(name);
            // SAFETY: 调用方在 guard 的整个生命周期内持有共享 env_lock。
            unsafe { std::env::set_var(name, value) };
            Self { name, previous }
        }
    }

    impl Drop for ScopedEnvOverride {
        fn drop(&mut self) {
            match self.previous.take() {
                // SAFETY: 调用方仍持有共享 env_lock，恢复操作不会与其他测试并发。
                Some(value) => unsafe { std::env::set_var(self.name, value) },
                // SAFETY: 同上。
                None => unsafe { std::env::remove_var(self.name) },
            }
        }
    }

    struct TestProvider {
        descriptor: crate::models::ProviderDescriptor,
    }

    struct DefaultTokenProvider {
        descriptor: crate::models::ProviderDescriptor,
    }

    struct CredentialCaptureProvider {
        descriptor: crate::models::ProviderDescriptor,
    }

    #[async_trait]
    impl AiProvider for TestProvider {
        fn descriptor(&self) -> crate::models::ProviderDescriptor {
            self.descriptor.clone()
        }

        async fn refresh(
            &self,
            _ctx: &ProviderExecutionContext<'_>,
        ) -> ProviderResult<crate::models::RefreshData> {
            Ok(crate::models::RefreshData::quotas_only(Vec::new()))
        }
    }

    impl ProviderCapabilities for TestProvider {
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
    }

    #[async_trait]
    impl AiProvider for DefaultTokenProvider {
        fn descriptor(&self) -> crate::models::ProviderDescriptor {
            self.descriptor.clone()
        }

        async fn refresh(
            &self,
            _ctx: &ProviderExecutionContext<'_>,
        ) -> ProviderResult<crate::models::RefreshData> {
            Ok(crate::models::RefreshData::quotas_only(Vec::new()))
        }
    }

    impl ProviderCapabilities for DefaultTokenProvider {
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
    }

    #[async_trait]
    impl AiProvider for CredentialCaptureProvider {
        fn descriptor(&self) -> crate::models::ProviderDescriptor {
            self.descriptor.clone()
        }

        async fn check_availability(
            &self,
            ctx: &ProviderExecutionContext<'_>,
        ) -> ProviderResult<()> {
            if ctx
                .provider_credentials
                .get_credential("test_token")
                .is_some()
            {
                Ok(())
            } else {
                Err(ProviderError::config_missing("test_token"))
            }
        }

        async fn refresh(
            &self,
            _ctx: &ProviderExecutionContext<'_>,
        ) -> ProviderResult<crate::models::RefreshData> {
            Ok(crate::models::RefreshData::quotas_only(Vec::new()))
        }
    }

    impl ProviderCapabilities for CredentialCaptureProvider {}

    #[test]
    fn test_all_provider_kinds_have_implementation() {
        let manager = ProviderManager::new();
        for kind in ProviderKind::all() {
            assert!(
                manager.providers_by_kind.contains_key(kind),
                "ProviderKind::{:?} is defined in models but NOT registered in ProviderManager.
                 Please check src/builtin_provider_manifest.rs and the matching provider module",
                kind
            );
        }
    }

    #[test]
    fn test_no_duplicate_provider_ids() {
        let manager = ProviderManager::new();
        let mut ids = std::collections::HashSet::new();
        for p in manager.providers_by_kind.values() {
            let id = p.descriptor().id;
            assert!(ids.insert(id.clone()), "Duplicate provider id: {}", id);
        }
        for id in manager.custom_providers_by_id.keys() {
            assert!(
                ids.insert(id.clone().into()),
                "Duplicate provider id: {}",
                id
            );
        }
    }

    #[test]
    fn test_no_duplicate_builtin_provider_kinds() {
        let manager = ProviderManager::new();
        // HashMap 的 key 天然保证唯一性，只需检查数量一致
        assert_eq!(
            manager.providers_by_kind.len(),
            ProviderKind::all().len(),
            "providers_by_kind count should match ProviderKind::all()"
        );
    }

    #[test]
    fn test_initial_statuses_sort_custom_providers_by_id() {
        fn custom_provider(id: &'static str) -> Arc<dyn ProviderEntry> {
            Arc::new(DefaultTokenProvider {
                descriptor: crate::models::ProviderDescriptor {
                    id: Cow::Borrowed(id),
                    metadata: ProviderMetadata {
                        kind: ProviderKind::Custom,
                        display_name: id.to_string(),
                        brand_name: id.to_string(),
                        icon_asset: String::new(),
                        dashboard_url: String::new(),
                        account_hint: String::new(),
                        source_label: String::new(),
                    },
                },
            })
        }

        let manager = ProviderManager::with_custom_providers([
            custom_provider("zeta:newapi"),
            custom_provider("alpha:newapi"),
            custom_provider("middle:newapi"),
        ]);

        let custom_ids: Vec<_> = manager
            .initial_statuses()
            .into_iter()
            .filter_map(|status| match status.provider_id {
                ProviderId::Custom(id) => Some(id),
                ProviderId::BuiltIn(_) => None,
            })
            .collect();

        assert_eq!(custom_ids, ["alpha:newapi", "middle:newapi", "zeta:newapi"]);
    }

    #[test]
    fn test_registration_rejects_custom_id_reserved_by_builtin_provider() {
        let provider: Arc<dyn ProviderEntry> = Arc::new(DefaultTokenProvider {
            descriptor: crate::models::ProviderDescriptor {
                id: Cow::Borrowed("gemini"),
                metadata: ProviderMetadata {
                    kind: ProviderKind::Custom,
                    display_name: "Conflicting Gemini".to_string(),
                    brand_name: "Conflicting Gemini".to_string(),
                    icon_asset: String::new(),
                    dashboard_url: String::new(),
                    account_hint: String::new(),
                    source_label: String::new(),
                },
            },
        });

        let manager = ProviderManager::with_custom_providers([provider]);

        assert!(manager.custom_provider_by_id("gemini").is_none());
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
        let mut manager = ProviderManager::empty();
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
        let mut manager = ProviderManager::empty();
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

    #[test]
    fn test_refresh_by_id_passes_provider_credentials() {
        let mut manager = ProviderManager::empty();
        manager.register(Arc::new(CredentialCaptureProvider {
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

        let mut credentials = ProviderSettings::default();
        credentials.set_credential("test_token", "configured-token".to_string());

        let result = smol::block_on(
            manager.refresh_by_id(&ProviderId::BuiltIn(ProviderKind::Amp), &credentials),
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_manager_handle_replaces_snapshot() {
        let handle = ProviderManagerHandle::new(ProviderManager::new());
        let original = handle.snapshot();
        let replacement = Arc::new(ProviderManager::new());

        handle.replace(replacement.clone());

        let current = handle.snapshot();
        assert!(Arc::ptr_eq(&current, &replacement));
        assert!(!Arc::ptr_eq(&current, &original));
    }

    #[test]
    fn default_manager_handle_does_not_load_custom_providers_from_disk() {
        let _guard = crate::utils::test_support::env_lock().lock().unwrap();
        let inherited_xdg_home = tempfile::tempdir().unwrap();
        let _inherited_xdg_home =
            ScopedEnvOverride::set("XDG_CONFIG_HOME", inherited_xdg_home.path());
        let temp_home = tempfile::tempdir().unwrap();
        let _home = ScopedEnvOverride::set("HOME", temp_home.path());
        let _xdg_home = ScopedEnvOverride::set("XDG_CONFIG_HOME", temp_home.path());

        let providers_dir = crate::platform::paths::custom_providers_dir();
        let xdg_home_is_isolated =
            std::env::var_os("XDG_CONFIG_HOME") == Some(temp_home.path().as_os_str().to_owned());
        let providers_dir_uses_inherited_xdg = providers_dir.starts_with(inherited_xdg_home.path());
        std::fs::create_dir_all(&providers_dir).unwrap();
        std::fs::write(
            providers_dir.join("disk-only.yaml"),
            r#"schema_version: 2
id: "disk-only:provider"
metadata:
  display_name: "Disk Only"
  brand_name: "Disk Only"
plan:
  steps:
    - name: placeholder
      availability:
        type: always
      source:
        type: placeholder
        reason: "test fixture"
"#,
        )
        .unwrap();

        let handle = ProviderManagerHandle::default();
        let custom_ids = handle.snapshot().custom_provider_ids();

        assert!(
            xdg_home_is_isolated,
            "test fixture must override an inherited XDG_CONFIG_HOME"
        );
        assert!(
            !providers_dir_uses_inherited_xdg,
            "test fixture must not write under inherited XDG_CONFIG_HOME"
        );
        assert!(
            custom_ids.is_empty(),
            "Default must be an in-memory manager; found disk providers: {custom_ids:?}"
        );
    }

    fn make_test_provider(id: &'static str, kind: ProviderKind) -> Arc<dyn ProviderEntry> {
        Arc::new(TestProvider {
            descriptor: crate::models::ProviderDescriptor {
                id: Cow::Borrowed(id),
                metadata: ProviderMetadata {
                    kind,
                    display_name: id.to_string(),
                    brand_name: id.to_string(),
                    icon_asset: String::new(),
                    dashboard_url: String::new(),
                    account_hint: String::new(),
                    source_label: String::new(),
                },
            },
        })
    }

    #[test]
    fn test_register_rejects_duplicate_id() {
        let mut manager = ProviderManager::empty();
        manager.register(make_test_provider("amp", ProviderKind::Amp));
        // 同 ID 不同 kind，应被拒绝
        manager.register(make_test_provider("amp", ProviderKind::Cursor));
        assert_eq!(manager.providers_by_kind.len(), 1);
        assert!(manager.providers_by_kind.contains_key(&ProviderKind::Amp));
    }

    #[test]
    fn test_register_rejects_duplicate_builtin_kind() {
        let mut manager = ProviderManager::empty();
        manager.register(make_test_provider("amp", ProviderKind::Amp));
        // 同 kind 不同 ID，应被拒绝
        manager.register(make_test_provider("amp2", ProviderKind::Amp));
        assert_eq!(manager.providers_by_kind.len(), 1);
    }

    #[test]
    #[should_panic(expected = "built-in provider manifest kind mismatch")]
    fn register_builtin_panics_when_manifest_kind_disagrees_with_descriptor() {
        let mut manager = ProviderManager::empty();
        manager.register_builtin(
            ProviderKind::Amp,
            make_test_provider("cursor:rotated", ProviderKind::Cursor),
        );
    }

    #[test]
    fn test_custom_provider_register_and_lookup() {
        let mut manager = ProviderManager::empty();
        manager.register(make_test_provider("my:custom", ProviderKind::Custom));

        assert!(manager.custom_provider_by_id("my:custom").is_some());
        assert!(manager.custom_provider_by_id("nonexistent").is_none());

        // provider_for_id 统一查找也能找到
        let found = manager.provider_for_id(&ProviderId::Custom("my:custom".to_string()));
        assert!(found.is_some());
    }

    #[test]
    fn test_metadata_for_fallback_when_kind_not_registered() {
        let manager = ProviderManager::empty();
        let meta = manager.metadata_for(ProviderKind::Amp);
        // 未注册时应返回 fallback
        assert_eq!(meta.kind, ProviderKind::Amp);
    }

    #[test]
    fn test_multiple_custom_providers_coexist() {
        let mut manager = ProviderManager::empty();
        manager.register(make_test_provider("custom:a", ProviderKind::Custom));
        manager.register(make_test_provider("custom:b", ProviderKind::Custom));
        assert_eq!(manager.custom_providers_by_id.len(), 2);
        assert_eq!(manager.custom_provider_ids().len(), 2);
    }
}
