//! Pure-logic application state, free of GPUI dependency.
//! Extracted for testability (GPUI proc macros crash during test compilation).

use crate::models::{
    AppSettings, ConnectionStatus, NavTab, ProviderId, ProviderKind, ProviderStatus,
};
use crate::notification::QuotaAlertTracker;

// ============================================================================
// Provider 面板可见性规则（单一真理来源，供 selector 和 popup_height 共用）
// ============================================================================

/// Provider 面板中各可选区域的可见性标志
pub struct ProviderPanelFlags {
    /// 是否显示账户信息卡片
    pub show_account_info: bool,
    /// 是否显示底部 Dashboard 链接行
    pub show_dashboard_row: bool,
    /// Provider 是否有 Dashboard URL
    pub has_dashboard_url: bool,
}

/// 根据设置和 Provider 状态计算面板可见性标志。
///
/// 核心规则：账户卡片已包含 Dashboard 入口时，隐藏底部 Dashboard 行（互斥）。
pub fn provider_panel_flags(
    settings: &AppSettings,
    provider: &ProviderStatus,
) -> ProviderPanelFlags {
    let has_dashboard_url = !provider.dashboard_url().is_empty();
    let show_account_info = settings.display.show_account_info && provider.account_email.is_some();
    let show_dashboard_row =
        settings.display.show_dashboard_button && has_dashboard_url && !show_account_info;

    ProviderPanelFlags {
        show_account_info,
        show_dashboard_row,
        has_dashboard_url,
    }
}

// ============================================================================
// 子状态结构 (SRP: 每个结构体负责一个独立职责)
// ============================================================================

/// Provider 数据存储
pub struct ProviderStore {
    pub providers: Vec<ProviderStatus>,
}

impl ProviderStore {
    /// 通过 ProviderId 查找 Provider
    pub fn find_by_id(&self, id: &ProviderId) -> Option<&ProviderStatus> {
        self.providers.iter().find(|p| p.provider_id == *id)
    }

    /// 通过 ProviderId 查找可变 Provider
    pub fn find_by_id_mut(&mut self, id: &ProviderId) -> Option<&mut ProviderStatus> {
        self.providers.iter_mut().find(|p| p.provider_id == *id)
    }

    /// 通过 ProviderId 标记为刷新中
    pub fn mark_refreshing_by_id(&mut self, id: &ProviderId) {
        if let Some(provider) = self.find_by_id_mut(id) {
            provider.mark_refreshing();
        }
    }

    /// 获取所有自定义 Provider 的 ID 列表
    pub fn custom_provider_ids(&self) -> Vec<ProviderId> {
        self.providers
            .iter()
            .filter(|p| p.provider_id.is_custom())
            .map(|p| p.provider_id.clone())
            .collect()
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
        let first_enabled = providers
            .iter()
            .find(|p| settings.is_enabled(&p.provider_id))
            .map(|p| p.provider_id.clone());

        let active_tab = first_enabled
            .clone()
            .map(NavTab::Provider)
            .unwrap_or(NavTab::Settings);

        Self {
            provider_store: ProviderStore { providers },
            nav: NavigationState {
                active_tab,
                last_provider_id: first_enabled
                    .unwrap_or(ProviderId::BuiltIn(ProviderKind::Claude)),
                generation: 0,
                prev_active_tab: None,
            },
            settings_ui: SettingsUiState {
                active_tab: SettingsTab::General,
                selected_provider: ProviderId::BuiltIn(ProviderKind::Claude),
                cadence_dropdown_open: false,
                copilot_token_editing: false,
                debug_selected_provider: None,
                debug_refresh_active: false,
                debug_prev_log_level: None,
            },
            settings,
            alert_tracker: QuotaAlertTracker::new(),
        }
    }

    pub fn header_status_text(&self) -> (String, HeaderStatusKind) {
        compute_header_status(&self.nav, &self.provider_store)
    }

    pub fn popup_height(&self) -> f32 {
        let id = if let NavTab::Provider(ref id) = self.nav.active_tab {
            id.clone()
        } else {
            self.nav.last_provider_id.clone()
        };
        let provider = self.provider_store.find_by_id(&id);
        let kind = id.kind();
        let quota_count = provider
            .map(|p| {
                let visible = self.settings.visible_quota_count(kind, &p.quotas);
                if visible == 0 && !p.quotas.is_empty() {
                    1 // 全部隐藏时显示空状态，至少预留 1 个卡片高度
                } else {
                    visible
                }
            })
            .unwrap_or(1);

        let (show_account, show_dashboard) = provider
            .map(|p| {
                let flags = provider_panel_flags(&self.settings, p);
                (flags.show_account_info, flags.show_dashboard_row)
            })
            .unwrap_or((false, false));

        crate::models::compute_popup_height_detailed(quota_count, show_dashboard, show_account)
    }

    pub fn has_enabled_providers(&self) -> bool {
        self.provider_store
            .providers
            .iter()
            .any(|p| self.settings.is_enabled(&p.provider_id))
    }

    pub fn default_provider_tab(&mut self) -> Option<NavTab> {
        if !self.has_enabled_providers() {
            return None;
        }

        let last = self.nav.last_provider_id.clone();
        let id = if self.settings.is_enabled(&last) {
            last
        } else {
            let fallback = self
                .provider_store
                .providers
                .iter()
                .find(|p| self.settings.is_enabled(&p.provider_id))
                .map(|p| p.provider_id.clone())
                .unwrap_or(last);
            self.nav.last_provider_id = fallback.clone();
            fallback
        };

        Some(NavTab::Provider(id))
    }
}

/// Tray 弹出窗口的导航状态
pub struct NavigationState {
    pub active_tab: NavTab,
    pub last_provider_id: ProviderId,
    /// 每次 switch_to 递增，用于让进度条动画在切换时重播
    pub generation: u64,
    /// 切换前的 tab，用于导航栏滑块动画的起点
    pub prev_active_tab: Option<NavTab>,
}

impl NavigationState {
    /// 切换到指定 tab，若为 Provider 则同步 last_provider_id
    pub fn switch_to(&mut self, tab: NavTab) {
        self.prev_active_tab = Some(self.active_tab.clone());
        self.generation += 1;
        if let NavTab::Provider(ref id) = tab {
            self.last_provider_id = id.clone();
        }
        self.active_tab = tab;
    }

    /// 当某个 provider 被禁用时，若它是当前活跃 tab 则回退到下一个已启用的 provider
    pub fn fallback_on_disable(
        &mut self,
        disabled: &ProviderId,
        providers: &[ProviderStatus],
        settings: &AppSettings,
    ) {
        let is_current = matches!(&self.active_tab, NavTab::Provider(id) if id == disabled);
        if !is_current {
            return;
        }
        if let Some(next) = providers
            .iter()
            .find(|p| p.provider_id != *disabled && settings.is_enabled(&p.provider_id))
        {
            self.switch_to(NavTab::Provider(next.provider_id.clone()));
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
    pub selected_provider: ProviderId,
    pub cadence_dropdown_open: bool,
    pub copilot_token_editing: bool,
    /// Debug Tab: 当前选中的调试 Provider
    pub debug_selected_provider: Option<ProviderId>,
    /// Debug Tab: 是否正在调试刷新中
    pub debug_refresh_active: bool,
    /// Debug Tab: 调试刷新前的日志级别（用于刷新完成后恢复）
    pub debug_prev_log_level: Option<log::LevelFilter>,
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
    let id = match &nav.active_tab {
        NavTab::Provider(id) => id.clone(),
        NavTab::Settings => nav.last_provider_id.clone(),
    };

    let Some(provider) = store.find_by_id(&id) else {
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
    use crate::models::{ConnectionStatus, DisplaySettings, ProviderId, ProviderKind};

    /// 快捷构造 ProviderId::BuiltIn
    fn pid(kind: ProviderKind) -> ProviderId {
        ProviderId::BuiltIn(kind)
    }

    fn make_provider(kind: ProviderKind) -> ProviderStatus {
        make_test_provider(kind, ConnectionStatus::Disconnected)
    }

    fn make_store(kinds: &[ProviderKind]) -> ProviderStore {
        ProviderStore {
            providers: kinds.iter().map(|k| make_provider(*k)).collect(),
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
        let store = make_store(&[ProviderKind::Claude, ProviderKind::Gemini]);
        assert!(store.find_by_id(&pid(ProviderKind::Claude)).is_some());
        assert!(store.find_by_id(&pid(ProviderKind::Gemini)).is_some());
    }

    #[test]
    fn store_find_missing() {
        let store = make_store(&[ProviderKind::Claude]);
        assert!(store.find_by_id(&pid(ProviderKind::Copilot)).is_none());
    }

    #[test]
    fn store_find_returns_correct_provider() {
        let store = make_store(&[
            ProviderKind::Claude,
            ProviderKind::Gemini,
            ProviderKind::Copilot,
        ]);
        let p = store.find_by_id(&pid(ProviderKind::Gemini)).unwrap();
        assert_eq!(p.kind(), ProviderKind::Gemini);
    }

    #[test]
    fn store_find_mut_modifies_connection() {
        let mut store = make_store(&[ProviderKind::Claude]);
        store
            .find_by_id_mut(&pid(ProviderKind::Claude))
            .unwrap()
            .connection = ConnectionStatus::Error;
        assert_eq!(
            store
                .find_by_id(&pid(ProviderKind::Claude))
                .unwrap()
                .connection,
            ConnectionStatus::Error
        );
    }

    #[test]
    fn store_find_mut_missing_returns_none() {
        let mut store = make_store(&[ProviderKind::Claude]);
        assert!(store.find_by_id_mut(&pid(ProviderKind::Copilot)).is_none());
    }

    #[test]
    fn store_mark_refreshing() {
        let mut store = make_store(&[ProviderKind::Claude]);
        assert_eq!(
            store
                .find_by_id(&pid(ProviderKind::Claude))
                .unwrap()
                .connection,
            ConnectionStatus::Disconnected
        );
        store.mark_refreshing_by_id(&pid(ProviderKind::Claude));
        assert_eq!(
            store
                .find_by_id(&pid(ProviderKind::Claude))
                .unwrap()
                .connection,
            ConnectionStatus::Refreshing
        );
    }

    #[test]
    fn store_mark_refreshing_missing_is_noop() {
        let mut store = make_store(&[ProviderKind::Claude]);
        // Should not panic
        store.mark_refreshing_by_id(&pid(ProviderKind::Copilot));
    }

    // ── NavigationState ────────────────────────────────────────

    #[test]
    fn nav_switch_to_provider() {
        let mut nav = NavigationState {
            active_tab: NavTab::Settings,
            last_provider_id: pid(ProviderKind::Claude),
            prev_active_tab: None,
            generation: 0,
        };
        nav.switch_to(NavTab::Provider(pid(ProviderKind::Gemini)));
        assert_eq!(nav.active_tab, NavTab::Provider(pid(ProviderKind::Gemini)));
        assert_eq!(nav.last_provider_id, pid(ProviderKind::Gemini));
    }

    #[test]
    fn nav_switch_to_settings_preserves_last_provider() {
        let mut nav = NavigationState {
            active_tab: NavTab::Provider(pid(ProviderKind::Claude)),
            last_provider_id: pid(ProviderKind::Claude),
            prev_active_tab: None,
            generation: 0,
        };
        nav.switch_to(NavTab::Settings);
        assert_eq!(nav.active_tab, NavTab::Settings);
        assert_eq!(nav.last_provider_id, pid(ProviderKind::Claude));
    }

    #[test]
    fn nav_switch_between_providers() {
        let mut nav = NavigationState {
            active_tab: NavTab::Provider(pid(ProviderKind::Claude)),
            last_provider_id: pid(ProviderKind::Claude),
            prev_active_tab: None,
            generation: 0,
        };
        nav.switch_to(NavTab::Provider(pid(ProviderKind::Gemini)));
        assert_eq!(nav.active_tab, NavTab::Provider(pid(ProviderKind::Gemini)));
        assert_eq!(nav.last_provider_id, pid(ProviderKind::Gemini));

        nav.switch_to(NavTab::Provider(pid(ProviderKind::Copilot)));
        assert_eq!(nav.last_provider_id, pid(ProviderKind::Copilot));
    }

    #[test]
    fn nav_fallback_when_current_disabled() {
        let store = make_store(&[ProviderKind::Claude, ProviderKind::Gemini]);
        let mut nav = NavigationState {
            active_tab: NavTab::Provider(pid(ProviderKind::Claude)),
            last_provider_id: pid(ProviderKind::Claude),
            prev_active_tab: None,
            generation: 0,
        };
        let settings = make_settings(&[ProviderKind::Claude, ProviderKind::Gemini]);
        nav.fallback_on_disable(&pid(ProviderKind::Claude), &store.providers, &settings);
        assert_eq!(nav.active_tab, NavTab::Provider(pid(ProviderKind::Gemini)));
        assert_eq!(nav.last_provider_id, pid(ProviderKind::Gemini));
    }

    #[test]
    fn nav_fallback_noop_when_not_current() {
        let store = make_store(&[ProviderKind::Gemini]);
        let mut nav = NavigationState {
            active_tab: NavTab::Provider(pid(ProviderKind::Gemini)),
            last_provider_id: pid(ProviderKind::Gemini),
            prev_active_tab: None,
            generation: 0,
        };
        let settings = make_settings(&[ProviderKind::Gemini]);
        nav.fallback_on_disable(&pid(ProviderKind::Claude), &store.providers, &settings);
        assert_eq!(nav.active_tab, NavTab::Provider(pid(ProviderKind::Gemini)));
        assert_eq!(nav.last_provider_id, pid(ProviderKind::Gemini));
    }

    #[test]
    fn nav_fallback_noop_when_on_settings_tab() {
        let store = make_store(&[ProviderKind::Gemini]);
        let mut nav = NavigationState {
            active_tab: NavTab::Settings,
            last_provider_id: pid(ProviderKind::Claude),
            prev_active_tab: None,
            generation: 0,
        };
        let settings = make_settings(&[ProviderKind::Gemini]);
        nav.fallback_on_disable(&pid(ProviderKind::Claude), &store.providers, &settings);
        assert_eq!(nav.active_tab, NavTab::Settings);
    }

    #[test]
    fn nav_fallback_no_other_enabled_stays_put() {
        let store = make_store(&[ProviderKind::Claude]);
        let mut nav = NavigationState {
            active_tab: NavTab::Provider(pid(ProviderKind::Claude)),
            last_provider_id: pid(ProviderKind::Claude),
            prev_active_tab: None,
            generation: 0,
        };
        let settings = make_settings(&[ProviderKind::Claude]);
        nav.fallback_on_disable(&pid(ProviderKind::Claude), &store.providers, &settings);
        assert_eq!(nav.active_tab, NavTab::Provider(pid(ProviderKind::Claude)));
        assert_eq!(nav.last_provider_id, pid(ProviderKind::Claude));
    }

    #[test]
    fn nav_fallback_picks_first_enabled_in_order() {
        let store = make_store(&[
            ProviderKind::Claude,
            ProviderKind::Gemini,
            ProviderKind::Copilot,
        ]);
        let mut nav = NavigationState {
            active_tab: NavTab::Provider(pid(ProviderKind::Claude)),
            last_provider_id: pid(ProviderKind::Claude),
            prev_active_tab: None,
            generation: 0,
        };
        let settings = make_settings(&[
            ProviderKind::Claude,
            ProviderKind::Gemini,
            ProviderKind::Copilot,
        ]);
        nav.fallback_on_disable(&pid(ProviderKind::Claude), &store.providers, &settings);
        assert_eq!(nav.active_tab, NavTab::Provider(pid(ProviderKind::Gemini)));
    }

    // ── SettingsUiState ────────────────────────────────────────

    #[test]
    fn settings_ui_default_values() {
        let ui = SettingsUiState {
            active_tab: SettingsTab::General,
            selected_provider: pid(ProviderKind::Claude),
            cadence_dropdown_open: false,
            copilot_token_editing: false,
            debug_selected_provider: None,
            debug_refresh_active: false,
            debug_prev_log_level: None,
        };
        assert_eq!(ui.active_tab, SettingsTab::General);
        assert!(!ui.cadence_dropdown_open);
    }

    // ── HeaderStatusText ────────────────────────────────────────

    #[test]
    fn header_status_missing_provider() {
        let store = make_store(&[]);
        let nav = NavigationState {
            active_tab: NavTab::Provider(pid(ProviderKind::Claude)),
            last_provider_id: pid(ProviderKind::Claude),
            prev_active_tab: None,
            generation: 0,
        };
        let (text, kind) = compute_header_status(&nav, &store);
        assert_eq!(text, "Offline");
        assert_eq!(kind, HeaderStatusKind::Offline);
    }

    #[test]
    fn header_status_refreshing() {
        let mut store = make_store(&[ProviderKind::Claude]);
        let p = store.find_by_id_mut(&pid(ProviderKind::Claude)).unwrap();
        p.connection = ConnectionStatus::Refreshing;

        let nav = NavigationState {
            active_tab: NavTab::Provider(pid(ProviderKind::Claude)),
            last_provider_id: pid(ProviderKind::Claude),
            prev_active_tab: None,
            generation: 0,
        };
        let (text, kind) = compute_header_status(&nav, &store);
        assert_eq!(text, "Syncing…");
        assert_eq!(kind, HeaderStatusKind::Syncing);
    }

    #[test]
    fn header_status_disconnected() {
        let mut store = make_store(&[ProviderKind::Claude]);
        let p = store.find_by_id_mut(&pid(ProviderKind::Claude)).unwrap();
        p.connection = ConnectionStatus::Disconnected;

        let nav = NavigationState {
            active_tab: NavTab::Provider(pid(ProviderKind::Claude)),
            last_provider_id: pid(ProviderKind::Claude),
            prev_active_tab: None,
            generation: 0,
        };
        let (text, kind) = compute_header_status(&nav, &store);
        assert_eq!(text, "Offline");
        assert_eq!(kind, HeaderStatusKind::Offline);
    }

    #[test]
    fn header_status_synced_now() {
        let mut store = make_store(&[ProviderKind::Claude]);
        let p = store.find_by_id_mut(&pid(ProviderKind::Claude)).unwrap();
        p.connection = ConnectionStatus::Connected;
        p.last_refreshed_instant = Some(std::time::Instant::now());

        let nav = NavigationState {
            active_tab: NavTab::Provider(pid(ProviderKind::Claude)),
            last_provider_id: pid(ProviderKind::Claude),
            prev_active_tab: None,
            generation: 0,
        };
        let (text, kind) = compute_header_status(&nav, &store);
        assert_eq!(text, "Synced");
        assert_eq!(kind, HeaderStatusKind::Synced);
    }

    #[test]
    fn header_status_synced_minutes_ago() {
        let mut store = make_store(&[ProviderKind::Claude]);
        let p = store.find_by_id_mut(&pid(ProviderKind::Claude)).unwrap();
        p.connection = ConnectionStatus::Connected;
        p.last_refreshed_instant =
            Some(std::time::Instant::now() - std::time::Duration::from_secs(300));

        let nav = NavigationState {
            active_tab: NavTab::Provider(pid(ProviderKind::Claude)),
            last_provider_id: pid(ProviderKind::Claude),
            prev_active_tab: None,
            generation: 0,
        };
        let (text, kind) = compute_header_status(&nav, &store);
        assert_eq!(text, "5m ago");
        assert_eq!(kind, HeaderStatusKind::Stale);
    }

    #[test]
    fn header_status_synced_hours_ago() {
        let mut store = make_store(&[ProviderKind::Claude]);
        let p = store.find_by_id_mut(&pid(ProviderKind::Claude)).unwrap();
        p.connection = ConnectionStatus::Connected;
        p.last_refreshed_instant =
            Some(std::time::Instant::now() - std::time::Duration::from_secs(7200));

        let nav = NavigationState {
            active_tab: NavTab::Provider(pid(ProviderKind::Claude)),
            last_provider_id: pid(ProviderKind::Claude),
            prev_active_tab: None,
            generation: 0,
        };
        let (text, kind) = compute_header_status(&nav, &store);
        assert_eq!(text, "2h ago");
        assert_eq!(kind, HeaderStatusKind::Stale);
    }

    // ── provider_panel_flags ──────────────────────────────────

    #[test]
    fn panel_flags_account_visible_hides_dashboard_row() {
        let settings = AppSettings {
            display: DisplaySettings {
                show_account_info: true,
                show_dashboard_button: true,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut provider = make_test_provider(ProviderKind::Gemini, ConnectionStatus::Connected);
        provider.account_email = Some("user@example.com".to_string());

        let flags = provider_panel_flags(&settings, &provider);
        assert!(flags.show_account_info);
        assert!(!flags.show_dashboard_row);
        assert!(flags.has_dashboard_url);
    }

    #[test]
    fn panel_flags_no_email_shows_dashboard_row() {
        let settings = AppSettings {
            display: DisplaySettings {
                show_account_info: true,
                show_dashboard_button: true,
                ..Default::default()
            },
            ..Default::default()
        };

        let provider = make_test_provider(ProviderKind::Gemini, ConnectionStatus::Connected);
        // account_email is None by default

        let flags = provider_panel_flags(&settings, &provider);
        assert!(!flags.show_account_info);
        assert!(flags.show_dashboard_row);
    }

    #[test]
    fn panel_flags_setting_off_shows_dashboard_row() {
        let settings = AppSettings {
            display: DisplaySettings {
                show_account_info: false,
                show_dashboard_button: true,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut provider = make_test_provider(ProviderKind::Gemini, ConnectionStatus::Connected);
        provider.account_email = Some("user@example.com".to_string());

        let flags = provider_panel_flags(&settings, &provider);
        assert!(!flags.show_account_info);
        assert!(flags.show_dashboard_row);
    }

    #[test]
    fn panel_flags_dashboard_setting_off() {
        let settings = AppSettings {
            display: DisplaySettings {
                show_account_info: true,
                show_dashboard_button: false,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut provider = make_test_provider(ProviderKind::Gemini, ConnectionStatus::Connected);
        provider.account_email = Some("user@example.com".to_string());

        let flags = provider_panel_flags(&settings, &provider);
        assert!(flags.show_account_info);
        assert!(!flags.show_dashboard_row);
        // dashboard_url 仍然存在（账户卡片 chevron 可用）
        assert!(flags.has_dashboard_url);
    }

    // ── HeaderStatusText ────────────────────────────────────────

    #[test]
    fn header_status_error() {
        let mut store = make_store(&[ProviderKind::Claude]);
        let p = store.find_by_id_mut(&pid(ProviderKind::Claude)).unwrap();
        p.connection = ConnectionStatus::Error;
        // 注意：如果是 Error 状态且 last_refreshed_instant 不为 None，
        // 我们会显示最后刷新时间（在前面分支处理了），所以这里设为 None 以测试 Error 分支
        p.last_refreshed_instant = None;

        let nav = NavigationState {
            active_tab: NavTab::Provider(pid(ProviderKind::Claude)),
            last_provider_id: pid(ProviderKind::Claude),
            prev_active_tab: None,
            generation: 0,
        };
        let (text, kind) = compute_header_status(&nav, &store);
        assert_eq!(text, "Error");
        assert_eq!(kind, HeaderStatusKind::Offline);
    }

    // ── ProviderStore: find_by_id / custom_provider_ids ──────

    #[test]
    fn store_find_by_id_builtin() {
        let store = make_store(&[ProviderKind::Claude]);
        assert!(store.find_by_id(&pid(ProviderKind::Claude)).is_some());
        assert!(store.find_by_id(&pid(ProviderKind::Gemini)).is_none());
    }

    #[test]
    fn store_find_by_id_custom() {
        let custom_id = ProviderId::Custom("myai:cli".to_string());
        let mut store = make_store(&[]);
        let metadata = crate::models::test_helpers::make_test_metadata(ProviderKind::Custom);
        store
            .providers
            .push(ProviderStatus::new_custom(custom_id.clone(), metadata));

        assert!(store.find_by_id(&custom_id).is_some());
        assert!(store.find_by_id(&pid(ProviderKind::Claude)).is_none());
    }

    #[test]
    fn store_find_by_id_mut_custom() {
        let custom_id = ProviderId::Custom("myai:cli".to_string());
        let mut store = make_store(&[]);
        let metadata = crate::models::test_helpers::make_test_metadata(ProviderKind::Custom);
        store
            .providers
            .push(ProviderStatus::new_custom(custom_id.clone(), metadata));

        store.find_by_id_mut(&custom_id).unwrap().connection = ConnectionStatus::Error;
        assert_eq!(
            store.find_by_id(&custom_id).unwrap().connection,
            ConnectionStatus::Error
        );
    }

    #[test]
    fn store_mark_refreshing_by_id_custom() {
        let custom_id = ProviderId::Custom("myai:cli".to_string());
        let mut store = make_store(&[]);
        let metadata = crate::models::test_helpers::make_test_metadata(ProviderKind::Custom);
        store
            .providers
            .push(ProviderStatus::new_custom(custom_id.clone(), metadata));

        store.mark_refreshing_by_id(&custom_id);
        assert_eq!(
            store.find_by_id(&custom_id).unwrap().connection,
            ConnectionStatus::Refreshing
        );
    }

    #[test]
    fn store_custom_provider_ids() {
        let custom1 = ProviderId::Custom("a:cli".to_string());
        let custom2 = ProviderId::Custom("b:cli".to_string());
        let metadata = crate::models::test_helpers::make_test_metadata(ProviderKind::Custom);
        let mut store = make_store(&[ProviderKind::Claude]);
        store.providers.push(ProviderStatus::new_custom(
            custom1.clone(),
            metadata.clone(),
        ));
        store
            .providers
            .push(ProviderStatus::new_custom(custom2.clone(), metadata));

        let ids = store.custom_provider_ids();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&custom1));
        assert!(ids.contains(&custom2));
    }

    #[test]
    fn store_custom_provider_ids_empty_when_no_custom() {
        let store = make_store(&[ProviderKind::Claude]);
        assert!(store.custom_provider_ids().is_empty());
    }
}
