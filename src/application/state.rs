//! Pure-logic application state, free of GPUI dependency.
//! Extracted for testability (GPUI proc macros crash during test compilation).

use super::quota_alert::QuotaAlertTracker;
use crate::models::{
    AppSettings, ConnectionStatus, NavTab, NewApiEditData, ProviderId, ProviderKind,
    ProviderStatus, StatusLevel,
};

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

    /// 按设置顺序迭代所有已启用的 Provider
    ///
    /// 集中了 "custom_ids → ordered → filter enabled → find_by_id" 的公共遍历模式，
    /// 供 `overview_view_state`、`DBusQuotaSnapshot::from_session` 等多处复用。
    pub fn enabled_providers<'a>(
        &'a self,
        settings: &'a super::super::models::AppSettings,
    ) -> impl Iterator<Item = &'a ProviderStatus> {
        let custom_ids = self.custom_provider_ids();
        // 将 ordered_ids 收集到 Vec，避免 lifetime 问题
        let ordered: Vec<_> = settings.provider.ordered_provider_ids(&custom_ids);
        ordered
            .into_iter()
            .filter(move |id| settings.provider.is_enabled(id))
            .filter_map(move |id| self.find_by_id(&id))
    }

    /// 根据新的状态列表同步自定义 Provider（热重载用）
    ///
    /// - 保留所有内置 Provider 状态不变
    /// - 新增的自定义 Provider 追加
    /// - 已删除的自定义 Provider 移除
    /// - 已存在的自定义 Provider 更新 definition（metadata + settings capability），保留运行时状态到下次刷新
    ///
    /// 返回新增或更新的自定义 Provider ID 列表（用于触发立即刷新）
    pub fn sync_custom_providers(&mut self, new_statuses: &[ProviderStatus]) -> Vec<ProviderId> {
        use std::collections::HashSet;

        let new_custom: Vec<_> = new_statuses
            .iter()
            .filter(|s| s.provider_id.is_custom())
            .collect();
        let new_custom_ids: HashSet<_> = new_custom.iter().map(|s| &s.provider_id).collect();

        // 移除已不存在的自定义 Provider
        self.providers
            .retain(|p| !p.provider_id.is_custom() || new_custom_ids.contains(&p.provider_id));

        let mut affected = Vec::new();
        for new_status in &new_custom {
            if let Some(existing) = self
                .providers
                .iter_mut()
                .find(|p| p.provider_id == new_status.provider_id)
            {
                // 已存在：同步 definition（metadata + settings capability），保留运行时状态
                if existing.sync_definition_from(new_status) {
                    affected.push(new_status.provider_id.clone());
                }
            } else {
                // 新增
                self.providers.push((*new_status).clone());
                affected.push(new_status.provider_id.clone());
            }
        }

        affected
    }
}

/// 纯逻辑应用会话状态
pub struct AppSession {
    pub provider_store: ProviderStore,
    pub nav: NavigationState,
    pub settings_ui: SettingsUiState,
    pub debug_ui: DebugUiState,
    pub settings: AppSettings,
    pub alert_tracker: QuotaAlertTracker,
    /// 弹窗是否可见（Dynamic 图标在弹窗可见时延迟更新，关闭后同步）
    pub popup_visible: bool,
}

impl AppSession {
    pub fn new(mut settings: AppSettings, providers: Vec<ProviderStatus>) -> Self {
        let provider_store = ProviderStore { providers };
        let custom_ids = provider_store.custom_provider_ids();

        // 自动注册已存在但未在 settings 中登记的自定义 Provider
        // （处理 YAML 文件存在但 settings.json 缺少对应条目的情况）
        settings
            .provider
            .register_discovered_custom_providers(&custom_ids);
        let nav = build_initial_navigation_state(&provider_store, &settings);
        let settings_ui = build_initial_settings_ui_state(&settings, &custom_ids);

        Self {
            provider_store,
            nav,
            settings_ui,
            debug_ui: DebugUiState::default(),
            settings,
            alert_tracker: QuotaAlertTracker::new(),
            popup_visible: false,
        }
    }

    pub fn header_status_text(&self) -> (HeaderStatusKind, Option<u64>) {
        compute_header_status(&self.nav, &self.provider_store)
    }

    pub fn popup_height(&self) -> f32 {
        compute_popup_height(&self.nav, &self.provider_store, &self.settings)
    }

    pub fn has_enabled_providers(&self) -> bool {
        self.provider_store
            .providers
            .iter()
            .any(|p| self.settings.provider.is_enabled(&p.provider_id))
    }

    pub fn default_provider_tab(&mut self) -> Option<NavTab> {
        if !self.has_enabled_providers() {
            return None;
        }

        let last = self.nav.last_provider_id.clone();
        let id = if self.settings.provider.is_enabled(&last) {
            last
        } else {
            let fallback = self
                .provider_store
                .providers
                .iter()
                .find(|p| self.settings.provider.is_enabled(&p.provider_id))
                .map(|p| p.provider_id.clone())
                .unwrap_or(last);
            self.nav.last_provider_id = fallback.clone();
            fallback
        };

        Some(NavTab::Provider(id))
    }

    /// 获取当前选中 Provider 的状态等级。
    /// 仅在 Dynamic 模式下使用，用于决定托盘图标颜色。
    ///
    /// 基于 `nav.last_provider_id`（当前/最后选中的 Provider）。
    /// 若该 Provider 未连接或不存在，返回 Green（安全默认值）。
    pub fn current_provider_status(&self) -> StatusLevel {
        self.provider_store
            .find_by_id(&self.nav.last_provider_id)
            .filter(|p| p.connection == ConnectionStatus::Connected)
            .map(|p| p.worst_status())
            .unwrap_or(StatusLevel::Green)
    }
}

fn build_initial_navigation_state(
    store: &ProviderStore,
    settings: &AppSettings,
) -> NavigationState {
    let first_enabled = first_enabled_provider_id(store, settings);
    let last_provider_id = first_enabled
        .clone()
        .unwrap_or_else(default_builtin_provider_id);
    let active_tab = initial_active_tab(settings, first_enabled);

    NavigationState {
        active_tab,
        last_provider_id,
        generation: 0,
        prev_active_tab: None,
    }
}

fn build_initial_settings_ui_state(
    settings: &AppSettings,
    custom_ids: &[ProviderId],
) -> SettingsUiState {
    // 设置页默认选中 sidebar 列表中的第一个 provider（而非硬编码 Claude）
    let selected_provider = settings
        .provider
        .sidebar_provider_ids(custom_ids)
        .into_iter()
        .next()
        .unwrap_or_else(default_builtin_provider_id);

    SettingsUiState {
        active_tab: SettingsTab::General,
        selected_provider,
        cadence_dropdown_open: false,
        token_editing_provider: None,
        adding_newapi: false,
        editing_newapi: None,
        adding_provider: false,
        confirming_remove_provider: false,
        confirming_delete_newapi: false,
        global_hotkey_error: None,
        global_hotkey_error_candidate: None,
    }
}

fn first_enabled_provider_id(store: &ProviderStore, settings: &AppSettings) -> Option<ProviderId> {
    store
        .providers
        .iter()
        .find(|p| settings.provider.is_enabled(&p.provider_id))
        .map(|p| p.provider_id.clone())
}

fn initial_active_tab(settings: &AppSettings, first_enabled: Option<ProviderId>) -> NavTab {
    if settings.display.show_overview {
        NavTab::Overview
    } else {
        first_enabled
            .map(NavTab::Provider)
            .unwrap_or(NavTab::Settings)
    }
}

fn default_builtin_provider_id() -> ProviderId {
    ProviderId::BuiltIn(ProviderKind::Claude)
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
            .find(|p| p.provider_id != *disabled && settings.provider.is_enabled(&p.provider_id))
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

/// 全局热键保存失败原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlobalHotkeyError {
    Empty,
    InvalidFormat,
    MissingModifier,
    ModifierOnly,
    Conflict(String),
    RegistrationFailed(String),
}

impl GlobalHotkeyError {
    /// 仅当配置本身不可用时返回 true；这类错误允许启动阶段回退到默认热键并修正磁盘。
    pub fn is_invalid_configuration(&self) -> bool {
        matches!(
            self,
            Self::Empty | Self::InvalidFormat | Self::MissingModifier | Self::ModifierOnly
        )
    }
}

/// 设置窗口的临时 UI 状态
pub struct SettingsUiState {
    pub active_tab: SettingsTab,
    pub selected_provider: ProviderId,
    pub cadence_dropdown_open: bool,
    /// 正在编辑 Token 的 Provider ID（None = 未编辑）
    pub token_editing_provider: Option<ProviderId>,
    /// 是否正在添加 NewAPI 中转站（右侧面板显示表单）
    pub adding_newapi: bool,
    /// 编辑模式：已有配置数据（Some = 编辑，None = 新增）
    pub editing_newapi: Option<NewApiEditData>,
    /// 是否正在选择要添加的 Provider（右侧面板显示选择列表）
    pub adding_provider: bool,
    /// 正在确认移除的 Provider（二次确认状态）
    pub confirming_remove_provider: bool,
    /// 正在确认删除的 NewAPI Provider（二次确认状态）
    pub confirming_delete_newapi: bool,
    /// General Tab 全局热键设置的最近一次错误
    pub global_hotkey_error: Option<GlobalHotkeyError>,
    /// 与 `global_hotkey_error` 对应的候选热键（持久化格式）
    pub global_hotkey_error_candidate: Option<String>,
}

/// Debug Tab 的临时 UI 状态（与主设置 UI 解耦）
#[derive(Default)]
pub struct DebugUiState {
    /// 当前选中的调试 Provider
    pub selected_provider: Option<ProviderId>,
    /// 是否正在调试刷新中
    pub refresh_active: bool,
    /// 调试刷新前的日志级别（用于刷新完成后恢复）
    pub prev_log_level: Option<log::LevelFilter>,
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

/// 根据当前导航状态和 Provider 数据计算弹出窗口高度
///
/// 步骤：解析当前 tab → 分派布局计算
pub fn compute_popup_height(
    nav: &NavigationState,
    store: &ProviderStore,
    settings: &AppSettings,
) -> f32 {
    // Overview 面板：根据已启用 Provider 数量计算高度（所有卡片默认折叠）
    if nav.active_tab == NavTab::Overview {
        let enabled_count = store
            .providers
            .iter()
            .filter(|p| settings.provider.is_enabled(&p.provider_id))
            .count();
        return crate::models::compute_popup_height_for_overview(enabled_count);
    }

    let id = match &nav.active_tab {
        NavTab::Provider(id) => id.clone(),
        _ => nav.last_provider_id.clone(),
    };
    let provider = store.find_by_id(&id);
    let quota_count = provider
        .map(|p| {
            let visible = settings.provider.visible_quota_count(&id, &p.quotas);
            if visible == 0 && !p.quotas.is_empty() {
                1 // 全部隐藏时显示空状态，至少预留 1 个卡片高度
            } else {
                visible
            }
        })
        .unwrap_or(1);

    let (show_account, show_dashboard) = provider
        .map(|p| {
            let flags = provider_panel_flags(settings, p);
            (flags.show_account_info, flags.show_dashboard_row)
        })
        .unwrap_or((false, false));

    crate::models::compute_popup_height_detailed(quota_count, show_dashboard, show_account)
}

/// 计算当前头部状态分类和可选的经过秒数
///
/// 返回 `(HeaderStatusKind, Option<elapsed_secs>)`，不做任何文本格式化。
/// 文本翻译和展示格式由 selector 层（`header_view_state`）负责。
pub fn compute_header_status(
    nav: &NavigationState,
    store: &ProviderStore,
) -> (HeaderStatusKind, Option<u64>) {
    let id = match &nav.active_tab {
        NavTab::Provider(id) => id.clone(),
        NavTab::Settings | NavTab::Overview => nav.last_provider_id.clone(),
    };

    let Some(provider) = store.find_by_id(&id) else {
        return (HeaderStatusKind::Offline, None);
    };

    if provider.connection == ConnectionStatus::Refreshing {
        return (HeaderStatusKind::Syncing, None);
    }

    if let Some(instant) = provider.last_refreshed_instant {
        let secs = instant.elapsed().as_secs();
        if secs < 60 {
            (HeaderStatusKind::Synced, Some(secs))
        } else {
            (HeaderStatusKind::Stale, Some(secs))
        }
    } else {
        match provider.connection {
            ConnectionStatus::Error => (HeaderStatusKind::Offline, None),
            ConnectionStatus::Disconnected => (HeaderStatusKind::Offline, None),
            _ => (HeaderStatusKind::Syncing, None),
        }
    }
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
