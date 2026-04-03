//! Pure-logic application state, free of GPUI dependency.
//! Extracted for testability (GPUI proc macros crash during test compilation).

use crate::models::{AppSettings, ConnectionStatus, NavTab, ProviderKind, ProviderStatus};
use crate::notification::QuotaAlertTracker;

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

/// 纯逻辑应用会话状态
pub struct AppSession {
    pub provider_store: ProviderStore,
    pub nav: NavigationState,
    pub settings_ui: SettingsUiState,
    pub settings: AppSettings,
    pub alert_tracker: QuotaAlertTracker,
}

impl AppSession {
    pub fn new(settings: AppSettings, providers: Vec<ProviderStatus>) -> Self {
        let first_enabled = ProviderKind::all()
            .iter()
            .find(|kind| settings.is_provider_enabled(**kind))
            .copied();

        let active_tab = first_enabled
            .map(NavTab::Provider)
            .unwrap_or(NavTab::Settings);

        Self {
            provider_store: ProviderStore { providers },
            nav: NavigationState {
                active_tab,
                last_provider_kind: first_enabled.unwrap_or(ProviderKind::Claude),
                generation: 0,
                prev_active_tab: None,
            },
            settings_ui: SettingsUiState {
                active_tab: SettingsTab::General,
                selected_provider: ProviderKind::Claude,
                cadence_dropdown_open: false,
                copilot_token_editing: false,
            },
            settings,
            alert_tracker: QuotaAlertTracker::new(),
        }
    }

    pub fn header_status_text(&self) -> (String, HeaderStatusKind) {
        compute_header_status(&self.nav, &self.provider_store)
    }

    pub fn popup_height(&self) -> f32 {
        let kind = if let NavTab::Provider(kind) = self.nav.active_tab {
            kind
        } else {
            self.nav.last_provider_kind
        };
        let provider = self.provider_store.find(kind);
        let quota_count = provider.map(|p| p.quotas.len()).unwrap_or(1);
        let has_dashboard = self.settings.show_dashboard_button
            && provider
                .map(|p| !p.dashboard_url().is_empty())
                .unwrap_or(false);

        crate::models::compute_popup_height_detailed(quota_count, has_dashboard)
    }

    pub fn has_enabled_providers(&self) -> bool {
        ProviderKind::all()
            .iter()
            .any(|kind| self.settings.is_provider_enabled(*kind))
    }

    pub fn default_provider_tab(&mut self) -> Option<NavTab> {
        if !self.has_enabled_providers() {
            return None;
        }

        let last = self.nav.last_provider_kind;
        let kind = if self.settings.is_provider_enabled(last) {
            last
        } else {
            let fallback = ProviderKind::all()
                .iter()
                .find(|kind| self.settings.is_provider_enabled(**kind))
                .copied()
                .unwrap_or(last);
            self.nav.last_provider_kind = fallback;
            fallback
        };

        Some(NavTab::Provider(kind))
    }
}

/// Tray 弹出窗口的导航状态
pub struct NavigationState {
    pub active_tab: NavTab,
    pub last_provider_kind: ProviderKind,
    /// 每次 switch_to 递增，用于让进度条动画在切换时重播
    pub generation: u64,
    /// 切换前的 tab，用于导航栏滑块动画的起点
    pub prev_active_tab: Option<NavTab>,
}

impl NavigationState {
    /// 切换到指定 tab，若为 Provider 则同步 last_provider_kind
    pub fn switch_to(&mut self, tab: NavTab) {
        self.prev_active_tab = Some(self.active_tab);
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
// 纯逻辑助手函数
// ============================================================================

/// 头部状态徽章类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderStatusKind {
    Synced,
    Syncing,
    Stale,
    Offline,
}

/// 计算当前头部应该显示的内容
/// < 1m: "● Synced", 1~59m: "● Xm ago", ≥ 1h: "● Xh ago"
/// Refreshing: "● Refreshing", 无数据: "● Offline"
pub fn compute_header_status(
    nav: &NavigationState,
    store: &ProviderStore,
) -> (String, HeaderStatusKind) {
    let kind = match nav.active_tab {
        NavTab::Provider(k) => k,
        NavTab::Settings => nav.last_provider_kind,
    };

    let Some(provider) = store.find(kind) else {
        return ("Offline".to_string(), HeaderStatusKind::Offline);
    };

    if provider.connection == ConnectionStatus::Refreshing {
        return ("Syncing…".to_string(), HeaderStatusKind::Syncing);
    }

    if let Some(instant) = provider.last_refreshed_instant {
        let secs = instant.elapsed().as_secs();
        if secs < 60 {
            ("Synced".to_string(), HeaderStatusKind::Synced)
        } else if secs < 3600 {
            (format!("{}m ago", secs / 60), HeaderStatusKind::Stale)
        } else {
            (format!("{}h ago", secs / 3600), HeaderStatusKind::Stale)
        }
    } else {
        match provider.connection {
            ConnectionStatus::Error => ("Error".to_string(), HeaderStatusKind::Offline),
            ConnectionStatus::Disconnected => ("Offline".to_string(), HeaderStatusKind::Offline),
            _ => ("Waiting".to_string(), HeaderStatusKind::Syncing),
        }
    }
}

// ============================================================================
// Tests (pure logic only, no GPUI dependency)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::test_helpers::make_test_provider;
    use crate::models::{ConnectionStatus, ProviderKind};

    fn make_provider(kind: ProviderKind, enabled: bool) -> ProviderStatus {
        let mut p = make_test_provider(kind, ConnectionStatus::Disconnected);
        p.enabled = enabled;
        p
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

    // ── HeaderStatusText ────────────────────────────────────────

    #[test]
    fn header_status_missing_provider() {
        let store = make_store(&[]);
        let nav = NavigationState {
            active_tab: NavTab::Provider(ProviderKind::Claude),
            last_provider_kind: ProviderKind::Claude,
            generation: 0,
        };
        let (text, kind) = compute_header_status(&nav, &store);
        assert_eq!(text, "Offline");
        assert_eq!(kind, HeaderStatusKind::Offline);
    }

    #[test]
    fn header_status_refreshing() {
        let mut store = make_store(&[(ProviderKind::Claude, true)]);
        let p = store.find_mut(ProviderKind::Claude).unwrap();
        p.connection = ConnectionStatus::Refreshing;

        let nav = NavigationState {
            active_tab: NavTab::Provider(ProviderKind::Claude),
            last_provider_kind: ProviderKind::Claude,
            generation: 0,
        };
        let (text, kind) = compute_header_status(&nav, &store);
        assert_eq!(text, "Syncing…");
        assert_eq!(kind, HeaderStatusKind::Syncing);
    }

    #[test]
    fn header_status_disconnected() {
        let mut store = make_store(&[(ProviderKind::Claude, true)]);
        let p = store.find_mut(ProviderKind::Claude).unwrap();
        p.connection = ConnectionStatus::Disconnected;

        let nav = NavigationState {
            active_tab: NavTab::Provider(ProviderKind::Claude),
            last_provider_kind: ProviderKind::Claude,
            generation: 0,
        };
        let (text, kind) = compute_header_status(&nav, &store);
        assert_eq!(text, "Offline");
        assert_eq!(kind, HeaderStatusKind::Offline);
    }

    #[test]
    fn header_status_synced_now() {
        let mut store = make_store(&[(ProviderKind::Claude, true)]);
        let p = store.find_mut(ProviderKind::Claude).unwrap();
        p.connection = ConnectionStatus::Connected;
        // 刚刷新的时间
        p.last_refreshed_instant = Some(std::time::Instant::now());

        let nav = NavigationState {
            active_tab: NavTab::Provider(ProviderKind::Claude),
            last_provider_kind: ProviderKind::Claude,
            generation: 0,
        };
        let (text, kind) = compute_header_status(&nav, &store);
        assert_eq!(text, "Synced");
        assert_eq!(kind, HeaderStatusKind::Synced);
    }

    #[test]
    fn header_status_synced_minutes_ago() {
        let mut store = make_store(&[(ProviderKind::Claude, true)]);
        let p = store.find_mut(ProviderKind::Claude).unwrap();
        p.connection = ConnectionStatus::Connected;
        // 5分钟前
        p.last_refreshed_instant =
            Some(std::time::Instant::now() - std::time::Duration::from_secs(300));

        let nav = NavigationState {
            active_tab: NavTab::Provider(ProviderKind::Claude),
            last_provider_kind: ProviderKind::Claude,
            generation: 0,
        };
        let (text, kind) = compute_header_status(&nav, &store);
        assert_eq!(text, "5m ago");
        assert_eq!(kind, HeaderStatusKind::Stale);
    }

    #[test]
    fn header_status_synced_hours_ago() {
        let mut store = make_store(&[(ProviderKind::Claude, true)]);
        let p = store.find_mut(ProviderKind::Claude).unwrap();
        p.connection = ConnectionStatus::Connected;
        // 2小时前
        p.last_refreshed_instant =
            Some(std::time::Instant::now() - std::time::Duration::from_secs(7200));

        let nav = NavigationState {
            active_tab: NavTab::Provider(ProviderKind::Claude),
            last_provider_kind: ProviderKind::Claude,
            generation: 0,
        };
        let (text, kind) = compute_header_status(&nav, &store);
        assert_eq!(text, "2h ago");
        assert_eq!(kind, HeaderStatusKind::Stale);
    }

    #[test]
    fn header_status_error() {
        let mut store = make_store(&[(ProviderKind::Claude, true)]);
        let p = store.find_mut(ProviderKind::Claude).unwrap();
        p.connection = ConnectionStatus::Error;
        // 注意：如果是 Error 状态且 last_refreshed_instant 不为 None，
        // 我们会显示最后刷新时间（在前面分支处理了），所以这里设为 None 以测试 Error 分支
        p.last_refreshed_instant = None;

        let nav = NavigationState {
            active_tab: NavTab::Provider(ProviderKind::Claude),
            last_provider_kind: ProviderKind::Claude,
            generation: 0,
        };
        let (text, kind) = compute_header_status(&nav, &store);
        assert_eq!(text, "Error");
        assert_eq!(kind, HeaderStatusKind::Offline);
    }
}
