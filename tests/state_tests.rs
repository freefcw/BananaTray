//! 集成测试 - 纯逻辑测试（避免 GPUI 宏干扰）
//!
//! 注意：这些测试被移到 tests/ 目录，因为 adabraka-gpui-macros
//! 在处理内联测试目标时会触发编译器 bug (SIGBUS)。

use std::sync::Arc;

// 只引入与 GPUI 无关的模型类型
use bananatray::models::{
    AppSettings, ConnectionStatus, ErrorKind, NavTab, ProviderId, ProviderKind, ProviderMetadata,
    ProviderStatus,
};

// 从 app/mod.rs 复制子状态结构（只复制数据部分，不涉及 GPUI）
pub struct ProviderStore {
    pub providers: Vec<ProviderStatus>,
    pub manager: Arc<bananatray::providers::ProviderManager>,
}

impl ProviderStore {
    pub fn find(&self, id: &ProviderId) -> Option<&ProviderStatus> {
        self.providers.iter().find(|p| &p.provider_id == id)
    }

    pub fn find_mut(&mut self, id: &ProviderId) -> Option<&mut ProviderStatus> {
        self.providers.iter_mut().find(|p| &p.provider_id == id)
    }

    pub fn set_connection(&mut self, id: &ProviderId, status: ConnectionStatus) {
        if let Some(p) = self.find_mut(id) {
            p.connection = status;
        }
    }
}

pub struct NavigationState {
    pub active_tab: NavTab,
    pub last_provider_id: ProviderId,
}

impl NavigationState {
    pub fn switch_to(&mut self, tab: NavTab) {
        self.active_tab = tab.clone();
        if let NavTab::Provider(id) = tab {
            self.last_provider_id = id;
        }
    }

    pub fn fallback_on_disable(&mut self, disabled: &ProviderId, settings: &AppSettings) {
        let is_current = matches!(self.active_tab, NavTab::Provider(ref k) if k == disabled);
        if !is_current {
            return;
        }
        if let Some(next) = ProviderKind::all()
            .iter()
            .map(|k| ProviderId::BuiltIn(*k))
            .find(|id| id != disabled && settings.is_enabled(id))
        {
            self.switch_to(NavTab::Provider(next));
        }
    }
}

// 测试辅助函数
fn make_provider(kind: ProviderKind, enabled: bool) -> ProviderStatus {
    let provider_id = ProviderId::BuiltIn(kind);
    ProviderStatus {
        provider_id,
        metadata: ProviderMetadata {
            kind,
            display_name: format!("{:?}", kind),
            brand_name: format!("{:?}", kind),
            source_label: "test".to_string(),
            account_hint: "test".to_string(),
            icon_asset: "test.svg".to_string(),
            dashboard_url: "https://example.com".to_string(),
        },
        enabled,
        connection: ConnectionStatus::Disconnected,
        quotas: vec![],
        account_email: None,
        is_paid: false,
        account_tier: None,
        last_updated_at: None,
        error_message: None,
        error_kind: ErrorKind::default(),
        last_refreshed_instant: None,
    }
}

fn make_store(kinds: &[(ProviderKind, bool)]) -> ProviderStore {
    ProviderStore {
        providers: kinds
            .iter()
            .map(|(k, enabled)| make_provider(*k, *enabled))
            .collect(),
        manager: Arc::new(bananatray::providers::ProviderManager::new()),
    }
}

fn provider_id(kind: ProviderKind) -> ProviderId {
    ProviderId::BuiltIn(kind)
}

fn make_settings(enabled: &[ProviderKind]) -> AppSettings {
    let mut s = AppSettings::default();
    for k in enabled {
        s.set_enabled(&ProviderId::BuiltIn(*k), true);
    }
    s
}

// ═════════════════════════════════════════════════════════════════════════════
// ProviderStore 测试
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn store_find_existing() {
    let store = make_store(&[(ProviderKind::Claude, true), (ProviderKind::Gemini, false)]);
    assert!(store.find(&provider_id(ProviderKind::Claude)).is_some());
    assert!(store.find(&provider_id(ProviderKind::Gemini)).is_some());
}

#[test]
fn store_find_missing() {
    let store = make_store(&[(ProviderKind::Claude, true)]);
    assert!(store.find(&provider_id(ProviderKind::Copilot)).is_none());
}

#[test]
fn store_find_mut_modifies() {
    let mut store = make_store(&[(ProviderKind::Claude, true)]);
    store
        .find_mut(&provider_id(ProviderKind::Claude))
        .unwrap()
        .enabled = false;
    assert!(
        !store
            .find(&provider_id(ProviderKind::Claude))
            .unwrap()
            .enabled
    );
}

#[test]
fn store_set_connection() {
    let mut store = make_store(&[(ProviderKind::Claude, true)]);
    assert_eq!(
        store
            .find(&provider_id(ProviderKind::Claude))
            .unwrap()
            .connection,
        ConnectionStatus::Disconnected
    );
    store.set_connection(
        &provider_id(ProviderKind::Claude),
        ConnectionStatus::Connected,
    );
    assert_eq!(
        store
            .find(&provider_id(ProviderKind::Claude))
            .unwrap()
            .connection,
        ConnectionStatus::Connected
    );
}

#[test]
fn store_set_connection_missing_is_noop() {
    let mut store = make_store(&[(ProviderKind::Claude, true)]);
    // Should not panic
    store.set_connection(&provider_id(ProviderKind::Copilot), ConnectionStatus::Error);
}

// ═════════════════════════════════════════════════════════════════════════════
// NavigationState 测试
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn nav_switch_to_provider() {
    let mut nav = NavigationState {
        active_tab: NavTab::Settings,
        last_provider_id: provider_id(ProviderKind::Claude),
    };
    nav.switch_to(NavTab::Provider(provider_id(ProviderKind::Gemini)));
    assert_eq!(
        nav.active_tab,
        NavTab::Provider(provider_id(ProviderKind::Gemini))
    );
    assert_eq!(nav.last_provider_id, provider_id(ProviderKind::Gemini));
}

#[test]
fn nav_switch_to_settings_preserves_last_provider() {
    let mut nav = NavigationState {
        active_tab: NavTab::Provider(provider_id(ProviderKind::Claude)),
        last_provider_id: provider_id(ProviderKind::Claude),
    };
    nav.switch_to(NavTab::Settings);
    assert_eq!(nav.active_tab, NavTab::Settings);
    assert_eq!(nav.last_provider_id, provider_id(ProviderKind::Claude));
}

#[test]
fn nav_fallback_when_current_disabled() {
    let mut nav = NavigationState {
        active_tab: NavTab::Provider(provider_id(ProviderKind::Claude)),
        last_provider_id: provider_id(ProviderKind::Claude),
    };
    let settings = make_settings(&[ProviderKind::Claude, ProviderKind::Gemini]);
    nav.fallback_on_disable(&provider_id(ProviderKind::Claude), &settings);
    assert_eq!(
        nav.active_tab,
        NavTab::Provider(provider_id(ProviderKind::Gemini))
    );
    assert_eq!(nav.last_provider_id, provider_id(ProviderKind::Gemini));
}

#[test]
fn nav_fallback_noop_when_not_current() {
    let mut nav = NavigationState {
        active_tab: NavTab::Provider(provider_id(ProviderKind::Gemini)),
        last_provider_id: provider_id(ProviderKind::Gemini),
    };
    let settings = make_settings(&[ProviderKind::Gemini]);
    nav.fallback_on_disable(&provider_id(ProviderKind::Claude), &settings);
    // Should not change
    assert_eq!(
        nav.active_tab,
        NavTab::Provider(provider_id(ProviderKind::Gemini))
    );
}

#[test]
fn nav_fallback_no_other_enabled() {
    let mut nav = NavigationState {
        active_tab: NavTab::Provider(provider_id(ProviderKind::Claude)),
        last_provider_id: provider_id(ProviderKind::Claude),
    };
    // No other providers enabled
    let settings = make_settings(&[ProviderKind::Claude]);
    nav.fallback_on_disable(&provider_id(ProviderKind::Claude), &settings);
    // Stays on Claude (no fallback available)
    assert_eq!(
        nav.active_tab,
        NavTab::Provider(provider_id(ProviderKind::Claude))
    );
}
