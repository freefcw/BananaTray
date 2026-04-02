//! Pure-logic sub-state structs, free of GPUI dependency.
//! Extracted for testability (GPUI proc macros crash during test compilation).

use crate::models::{AppSettings, NavTab, ProviderKind, ProviderStatus};

// ============================================================================
// 子状态结构 (SRP: 每个结构体负责一个独立职责)
// ============================================================================

/// Provider 数据存储
pub struct ProviderStore {
    pub providers: Vec<ProviderStatus>,
}

impl ProviderStore {
    pub fn find(&self, kind: ProviderKind) -> Option<&ProviderStatus> {
        self.providers.iter().find(|p| p.kind == kind)
    }

    pub fn find_mut(&mut self, kind: ProviderKind) -> Option<&mut ProviderStatus> {
        self.providers.iter_mut().find(|p| p.kind == kind)
    }

    pub fn mark_refreshing(&mut self, kind: ProviderKind) {
        if let Some(provider) = self.find_mut(kind) {
            provider.mark_refreshing();
        }
    }
}

/// Tray 弹出窗口的导航状态
pub struct NavigationState {
    pub active_tab: NavTab,
    pub last_provider_kind: ProviderKind,
    /// 每次 switch_to 递增，用于让进度条动画在切换时重播
    pub generation: u64,
}

impl NavigationState {
    /// 切换到指定 tab，若为 Provider 则同步 last_provider_kind
    pub fn switch_to(&mut self, tab: NavTab) {
        self.generation += 1;
        self.active_tab = tab;
        if let NavTab::Provider(kind) = tab {
            self.last_provider_kind = kind;
        }
    }

    /// 当某个 provider 被禁用时，若它是当前活跃 tab 则回退到下一个已启用的 provider
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

/// 设置窗口 Tab 枚举（纯数据，不依赖 GPUI）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    General,
    Providers,
    Display,
    About,
    Debug,
}

/// 设置窗口的临时 UI 状态
pub struct SettingsUiState {
    pub active_tab: SettingsTab,
    pub selected_provider: ProviderKind,
    pub cadence_dropdown_open: bool,
    pub copilot_token_editing: bool,
}

// ============================================================================
// Tests (pure logic only, no GPUI dependency)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        ConnectionStatus, ErrorKind, ProviderKind, ProviderMetadata, ProviderStatus,
    };

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
        }
    }

    fn make_settings(enabled: &[ProviderKind]) -> AppSettings {
        let mut s = AppSettings::default();
        for k in enabled {
            s.set_provider_enabled(*k, true);
        }
        s
    }

    // ── ProviderStore ──────────────────────────────────────────

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
    fn store_find_returns_correct_provider() {
        let store = make_store(&[
            (ProviderKind::Claude, true),
            (ProviderKind::Gemini, false),
            (ProviderKind::Copilot, true),
        ]);
        let p = store.find(ProviderKind::Gemini).unwrap();
        assert_eq!(p.kind, ProviderKind::Gemini);
        assert!(!p.enabled);
    }

    #[test]
    fn store_find_mut_modifies() {
        let mut store = make_store(&[(ProviderKind::Claude, true)]);
        store.find_mut(ProviderKind::Claude).unwrap().enabled = false;
        assert!(!store.find(ProviderKind::Claude).unwrap().enabled);
    }

    #[test]
    fn store_find_mut_missing_returns_none() {
        let mut store = make_store(&[(ProviderKind::Claude, true)]);
        assert!(store.find_mut(ProviderKind::Copilot).is_none());
    }

    #[test]
    fn store_mark_refreshing() {
        let mut store = make_store(&[(ProviderKind::Claude, true)]);
        assert_eq!(
            store.find(ProviderKind::Claude).unwrap().connection,
            ConnectionStatus::Disconnected
        );
        store.mark_refreshing(ProviderKind::Claude);
        assert_eq!(
            store.find(ProviderKind::Claude).unwrap().connection,
            ConnectionStatus::Refreshing
        );
    }

    #[test]
    fn store_mark_refreshing_missing_is_noop() {
        let mut store = make_store(&[(ProviderKind::Claude, true)]);
        // Should not panic
        store.mark_refreshing(ProviderKind::Copilot);
    }

    // ── NavigationState ────────────────────────────────────────

    #[test]
    fn nav_switch_to_provider() {
        let mut nav = NavigationState {
            active_tab: NavTab::Settings,
            last_provider_kind: ProviderKind::Claude,
            generation: 0,
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
            generation: 0,
        };
        nav.switch_to(NavTab::Settings);
        assert_eq!(nav.active_tab, NavTab::Settings);
        // last_provider_kind should remain unchanged
        assert_eq!(nav.last_provider_kind, ProviderKind::Claude);
    }

    #[test]
    fn nav_switch_between_providers() {
        let mut nav = NavigationState {
            active_tab: NavTab::Provider(ProviderKind::Claude),
            last_provider_kind: ProviderKind::Claude,
            generation: 0,
        };
        nav.switch_to(NavTab::Provider(ProviderKind::Gemini));
        assert_eq!(nav.active_tab, NavTab::Provider(ProviderKind::Gemini));
        assert_eq!(nav.last_provider_kind, ProviderKind::Gemini);

        nav.switch_to(NavTab::Provider(ProviderKind::Copilot));
        assert_eq!(nav.last_provider_kind, ProviderKind::Copilot);
    }

    #[test]
    fn nav_fallback_when_current_disabled() {
        let mut nav = NavigationState {
            active_tab: NavTab::Provider(ProviderKind::Claude),
            last_provider_kind: ProviderKind::Claude,
            generation: 0,
        };
        let settings = make_settings(&[ProviderKind::Claude, ProviderKind::Gemini]);
        nav.fallback_on_disable(ProviderKind::Claude, &settings);
        // Should fall back to next enabled provider (Gemini)
        assert_eq!(nav.active_tab, NavTab::Provider(ProviderKind::Gemini));
        assert_eq!(nav.last_provider_kind, ProviderKind::Gemini);
    }

    #[test]
    fn nav_fallback_noop_when_not_current() {
        let mut nav = NavigationState {
            active_tab: NavTab::Provider(ProviderKind::Gemini),
            last_provider_kind: ProviderKind::Gemini,
            generation: 0,
        };
        let settings = make_settings(&[ProviderKind::Gemini]);
        nav.fallback_on_disable(ProviderKind::Claude, &settings);
        // Should not change since Claude is not the active tab
        assert_eq!(nav.active_tab, NavTab::Provider(ProviderKind::Gemini));
        assert_eq!(nav.last_provider_kind, ProviderKind::Gemini);
    }

    #[test]
    fn nav_fallback_noop_when_on_settings_tab() {
        let mut nav = NavigationState {
            active_tab: NavTab::Settings,
            last_provider_kind: ProviderKind::Claude,
            generation: 0,
        };
        let settings = make_settings(&[ProviderKind::Gemini]);
        nav.fallback_on_disable(ProviderKind::Claude, &settings);
        // Settings tab should remain
        assert_eq!(nav.active_tab, NavTab::Settings);
    }

    #[test]
    fn nav_fallback_no_other_enabled_stays_put() {
        let mut nav = NavigationState {
            active_tab: NavTab::Provider(ProviderKind::Claude),
            last_provider_kind: ProviderKind::Claude,
            generation: 0,
        };
        // Only Claude is enabled, and we're disabling it — no fallback target
        let settings = make_settings(&[ProviderKind::Claude]);
        nav.fallback_on_disable(ProviderKind::Claude, &settings);
        // Stays on Claude (no alternative available)
        assert_eq!(nav.active_tab, NavTab::Provider(ProviderKind::Claude));
        assert_eq!(nav.last_provider_kind, ProviderKind::Claude);
    }

    #[test]
    fn nav_fallback_picks_first_enabled_in_order() {
        let mut nav = NavigationState {
            active_tab: NavTab::Provider(ProviderKind::Claude),
            last_provider_kind: ProviderKind::Claude,
            generation: 0,
        };
        // Enable Copilot and Gemini; fallback should pick based on ProviderKind::all() order
        let settings = make_settings(&[
            ProviderKind::Claude,
            ProviderKind::Gemini,
            ProviderKind::Copilot,
        ]);
        nav.fallback_on_disable(ProviderKind::Claude, &settings);
        // Gemini comes before Copilot in ProviderKind::all()
        assert_eq!(nav.active_tab, NavTab::Provider(ProviderKind::Gemini));
    }

    // ── SettingsUiState ────────────────────────────────────────

    #[test]
    fn settings_ui_default_values() {
        let ui = SettingsUiState {
            active_tab: SettingsTab::General,
            selected_provider: ProviderKind::Claude,
            cadence_dropdown_open: false,
            copilot_token_editing: false,
        };
        assert_eq!(ui.active_tab, SettingsTab::General);
        assert!(!ui.cadence_dropdown_open);
    }
}
