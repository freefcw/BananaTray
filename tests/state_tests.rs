//! 集成测试 - 纯逻辑测试（避免 GPUI 宏干扰）
//!
//! 注意：这些测试被移到 tests/ 目录，因为 adabraka-gpui-macros
//! 在处理内联测试目标时会触发编译器 bug (SIGBUS)。

use std::sync::Arc;

// 只引入与 GPUI 无关的模型类型
use bananatray::models::{
    AppSettings, ConnectionStatus, NavTab, ProviderKind, ProviderMetadata, ProviderStatus,
};

// 从 app/mod.rs 复制子状态结构（只复制数据部分，不涉及 GPUI）
pub struct ProviderStore {
    pub providers: Vec<ProviderStatus>,
    pub manager: Arc<bananatray::providers::ProviderManager>,
    pub last_refresh_started: Option<std::time::Instant>,
}

impl ProviderStore {
    pub fn find(&self, kind: ProviderKind) -> Option<&ProviderStatus> {
        self.providers.iter().find(|p| p.kind == kind)
    }

    pub fn find_mut(&mut self, kind: ProviderKind) -> Option<&mut ProviderStatus> {
        self.providers.iter_mut().find(|p| p.kind == kind)
    }

    pub fn set_connection(&mut self, kind: ProviderKind, status: ConnectionStatus) {
        if let Some(p) = self.find_mut(kind) {
            p.connection = status;
        }
    }
}

pub struct NavigationState {
    pub active_tab: NavTab,
    pub last_provider_kind: ProviderKind,
}

impl NavigationState {
    pub fn switch_to(&mut self, tab: NavTab) {
        self.active_tab = tab;
        if let NavTab::Provider(kind) = tab {
            self.last_provider_kind = kind;
        }
    }

    pub fn fallback_on_disable(&mut self, disabled: ProviderKind, settings: &AppSettings) {
        let is_current = matches!(self.active_tab, NavTab::Provider(k) if k == disabled);
        if !is_current {
            return;
        }
        if let Some(next) = ProviderKind::all()
            .iter()
            .find(|k| **k != disabled && settings.is_provider_enabled(**k))
            .copied()
        {
            self.switch_to(NavTab::Provider(next));
        }
    }
}

pub struct SettingsUiState {
    pub active_tab: SettingsTab,
    pub selected_provider: ProviderKind,
    pub cadence_dropdown_open: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsTab {
    General,
    Providers,
}

// 测试辅助函数
fn make_provider(kind: ProviderKind, enabled: bool) -> ProviderStatus {
    ProviderStatus {
        kind,
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
        last_refresh_started: None,
    }
}

fn make_settings(enabled: &[ProviderKind]) -> AppSettings {
    let mut s = AppSettings::default();
    for k in enabled {
        s.set_provider_enabled(*k, true);
    }
    s
}

// ═════════════════════════════════════════════════════════════════════════════
// ProviderStore 测试
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn store_find_existing() {
    let store = make_store(&[(ProviderKind::Claude, true), (ProviderKind::Gemini, false)]);
    assert!(store.find(ProviderKind::Claude).is_some());
    assert!(store.find(ProviderKind::Gemini).is_some());
}

#[test]
fn store_find_missing() {
    let store = make_store(&[(ProviderKind::Claude, true)]);
    assert!(store.find(ProviderKind::Copilot).is_none());
}

#[test]
fn store_find_mut_modifies() {
    let mut store = make_store(&[(ProviderKind::Claude, true)]);
    store.find_mut(ProviderKind::Claude).unwrap().enabled = false;
    assert!(!store.find(ProviderKind::Claude).unwrap().enabled);
}

#[test]
fn store_set_connection() {
    let mut store = make_store(&[(ProviderKind::Claude, true)]);
    assert_eq!(
        store.find(ProviderKind::Claude).unwrap().connection,
        ConnectionStatus::Disconnected
    );
    store.set_connection(ProviderKind::Claude, ConnectionStatus::Connected);
    assert_eq!(
        store.find(ProviderKind::Claude).unwrap().connection,
        ConnectionStatus::Connected
    );
}

#[test]
fn store_set_connection_missing_is_noop() {
    let mut store = make_store(&[(ProviderKind::Claude, true)]);
    // Should not panic
    store.set_connection(ProviderKind::Copilot, ConnectionStatus::Error);
}

// ═════════════════════════════════════════════════════════════════════════════
// NavigationState 测试
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn nav_switch_to_provider() {
    let mut nav = NavigationState {
        active_tab: NavTab::Settings,
        last_provider_kind: ProviderKind::Claude,
    };
    nav.switch_to(NavTab::Provider(ProviderKind::Gemini));
    assert_eq!(nav.active_tab, NavTab::Provider(ProviderKind::Gemini));
    assert_eq!(nav.last_provider_kind, ProviderKind::Gemini);
}

#[test]
fn nav_switch_to_settings_preserves_last_provider() {
    let mut nav = NavigationState {
        active_tab: NavTab::Provider(ProviderKind::Claude),
        last_provider_kind: ProviderKind::Claude,
    };
    nav.switch_to(NavTab::Settings);
    assert_eq!(nav.active_tab, NavTab::Settings);
    assert_eq!(nav.last_provider_kind, ProviderKind::Claude);
}

#[test]
fn nav_fallback_when_current_disabled() {
    let mut nav = NavigationState {
        active_tab: NavTab::Provider(ProviderKind::Claude),
        last_provider_kind: ProviderKind::Claude,
    };
    let settings = make_settings(&[ProviderKind::Claude, ProviderKind::Gemini]);
    nav.fallback_on_disable(ProviderKind::Claude, &settings);
    assert_eq!(nav.active_tab, NavTab::Provider(ProviderKind::Gemini));
    assert_eq!(nav.last_provider_kind, ProviderKind::Gemini);
}

#[test]
fn nav_fallback_noop_when_not_current() {
    let mut nav = NavigationState {
        active_tab: NavTab::Provider(ProviderKind::Gemini),
        last_provider_kind: ProviderKind::Gemini,
    };
    let settings = make_settings(&[ProviderKind::Gemini]);
    nav.fallback_on_disable(ProviderKind::Claude, &settings);
    // Should not change
    assert_eq!(nav.active_tab, NavTab::Provider(ProviderKind::Gemini));
}

#[test]
fn nav_fallback_no_other_enabled() {
    let mut nav = NavigationState {
        active_tab: NavTab::Provider(ProviderKind::Claude),
        last_provider_kind: ProviderKind::Claude,
    };
    // No other providers enabled
    let settings = make_settings(&[ProviderKind::Claude]);
    nav.fallback_on_disable(ProviderKind::Claude, &settings);
    // Stays on Claude (no fallback available)
    assert_eq!(nav.active_tab, NavTab::Provider(ProviderKind::Claude));
}
