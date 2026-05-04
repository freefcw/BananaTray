#[cfg(any(target_os = "linux", test))]
mod dbus_dto;
mod debug;
mod format;
mod issue_report;
mod settings;
mod tray;

// ============================================================================
// ViewModel 类型定义（所有 selector 的共享产出物）
// ============================================================================

use super::state::HeaderStatusKind;
use crate::models::{
    NewApiEditData, ProviderCapability, ProviderId, QuotaDisplayMode, QuotaInfo,
    SettingsCapability, StatusLevel,
};

// ── Tray 弹出窗口 ──

#[derive(Debug, Clone)]
pub struct HeaderViewState {
    pub status_text: String,
    pub status_kind: HeaderStatusKind,
}

#[derive(Debug, Clone)]
pub struct GlobalActionsViewState {
    pub show_refresh: bool,
    pub refresh: RefreshButtonViewState,
}

/// 刷新按钮的目标：单个 Provider 或全部
#[derive(Debug, Clone)]
pub enum RefreshTarget {
    /// 刷新指定 Provider（Provider 详情页）
    One(ProviderId),
    /// 刷新所有已启用 Provider（Overview 页）
    All,
}

#[derive(Debug, Clone)]
pub struct RefreshButtonViewState {
    /// 刷新目标（None = 无可刷新目标，如 Settings 页）
    pub target: Option<RefreshTarget>,
    pub is_refreshing: bool,
    pub label: String,
}

#[derive(Debug, Clone)]
pub enum ProviderDetailViewState {
    Disabled(DisabledProviderViewState),
    Missing { message: String },
    Panel(ProviderPanelViewState),
}

#[derive(Debug, Clone)]
pub struct DisabledProviderViewState {
    pub id: ProviderId,
    pub icon: String,
    pub title: String,
    pub hint: String,
}

#[derive(Debug, Clone)]
pub struct ProviderPanelViewState {
    pub id: ProviderId,
    pub show_dashboard: bool,
    pub account: Option<AccountInfoViewState>,
    pub body: ProviderBodyViewState,
    pub quota_display_mode: QuotaDisplayMode,
}

/// 账户信息卡片 ViewModel
#[derive(Debug, Clone)]
pub struct AccountInfoViewState {
    /// 账户邮箱
    pub email: String,
    /// 套餐名称（如 "Pro", "Max"）
    pub tier: Option<String>,
    /// 上次更新时间描述
    pub updated_text: String,
    /// 可打开的 Dashboard 链接（None 表示不可点击）
    pub dashboard_url: Option<String>,
}

#[derive(Debug, Clone)]
pub enum ProviderBodyViewState {
    Refreshing {
        provider_name: String,
    },
    Quotas {
        quotas: Vec<QuotaDisplayViewState>,
        generation: u64,
    },
    Empty(ProviderEmptyViewState),
}

#[derive(Debug, Clone)]
pub struct ProviderEmptyViewState {
    pub id: ProviderId,
    pub title: String,
    pub message: String,
    pub is_error: bool,
    pub action: Option<ProviderEmptyAction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderEmptyAction {
    OpenSettings,
    RetryRefresh,
}

// ── Overview 总览面板 ──

/// Overview 总览面板 ViewModel
#[derive(Debug, Clone)]
pub struct OverviewViewState {
    pub items: Vec<OverviewItemViewState>,
}

/// 单个 Provider 在 Overview 中的紧凑展示
#[derive(Debug, Clone)]
pub struct OverviewItemViewState {
    pub id: ProviderId,
    pub icon: String,
    pub display_name: String,
    pub status: OverviewItemStatus,
}

/// Overview 单项的状态展示
#[derive(Debug, Clone)]
pub enum OverviewItemStatus {
    /// 有有效配额数据（所有展示值已由 selector 预计算）
    Quota {
        /// 总体最差状态等级（用于状态点和徽章颜色）
        status_level: StatusLevel,
        /// 所有可见配额（按 status_level 降序，最差的在前）
        quotas: Vec<OverviewQuotaItem>,
    },
    /// 正在刷新
    Refreshing,
    /// 错误或无数据
    Error { message: String },
    /// 未连接
    Disconnected,
}

/// Overview 中单个配额的预计算展示数据
#[derive(Debug, Clone)]
pub struct OverviewQuotaItem {
    /// 配额名称（如 "Session"、"Weekly"）
    pub label: String,
    /// 预计算的显示文本（如 "70%"、"$15.00"）
    pub display_text: String,
    /// 进度条比例 [0.0, 1.0]
    pub bar_ratio: f32,
    /// 此配额的状态等级
    pub status_level: StatusLevel,
}

/// 单个配额的展示 ViewModel。
///
/// `quota` 保留数值语义与状态计算能力，`label/detail` 则由 selector 基于当前 locale
/// 生成，避免把最终展示字符串缓存在 `ProviderStatus` 中。
#[derive(Debug, Clone)]
pub struct QuotaDisplayViewState {
    pub quota: QuotaInfo,
    pub label: String,
    pub detail: String,
}

// ── Settings 窗口 ──

#[derive(Debug, Clone)]
pub struct SettingsProvidersTabViewState {
    pub items: Vec<SettingsProviderListItemViewState>,
    pub detail: SettingsProviderDetailViewState,
    /// 是否正在添加新 Provider（右侧面板显示 NewAPI 表单）
    pub adding_newapi: bool,
    /// 编辑模式：已有配置数据（Some = 编辑，None = 新增）
    pub editing_newapi_data: Option<NewApiEditData>,
    /// 是否处于"添加 Provider"选择模式
    pub adding_provider: bool,
    /// 可添加的 Provider 列表（供选择面板使用）
    pub available_providers: Vec<AvailableProviderItem>,
}

/// 可添加到 sidebar 的 Provider 项
#[derive(Debug, Clone)]
pub struct AvailableProviderItem {
    pub id: ProviderId,
    pub icon: String,
    pub display_name: String,
}

#[derive(Debug, Clone)]
pub struct SettingsProviderListItemViewState {
    pub id: ProviderId,
    pub icon: String,
    pub display_name: String,
    pub is_selected: bool,
    pub is_enabled: bool,
}

#[derive(Debug, Clone)]
pub struct SettingsProviderDetailViewState {
    pub id: ProviderId,
    pub icon: String,
    pub display_name: String,
    pub subtitle: String,
    pub is_enabled: bool,
    pub can_refresh: bool,
    pub show_quota_visibility: bool,
    pub provider_capability: ProviderCapability,
    pub info: SettingsProviderInfoViewState,
    pub usage: SettingsProviderUsageViewState,
    pub settings_capability: SettingsCapability,
    pub quota_display_mode: QuotaDisplayMode,
    /// 配额可见性列表（用于设置 UI 中的勾选框）
    pub quota_visibility: Vec<QuotaVisibilityItem>,
}

/// 单个配额在托盘弹窗中的可见性状态
#[derive(Debug, Clone)]
pub struct QuotaVisibilityItem {
    /// 配额标签（i18n，仅用于 UI 显示）
    pub label: String,
    /// 语言无关的稳定标识符（来自 QuotaType::stable_key()，用于持久化和 action 传递）
    pub quota_key: String,
    /// 是否在托盘弹窗中显示
    pub visible: bool,
}

#[derive(Debug, Clone)]
pub struct SettingsProviderInfoViewState {
    pub state_text: String,
    pub source_text: String,
    pub updated_text: String,
    pub status_text: String,
    pub status_kind: SettingsProviderStatusKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsProviderStatusKind {
    Neutral,
    Success,
    Error,
}

#[derive(Debug, Clone)]
pub enum SettingsProviderUsageViewState {
    Disabled { message: String },
    Quotas { quotas: Vec<QuotaDisplayViewState> },
    Error { title: String, message: String },
    Empty { message: String },
    Missing { message: String },
}

// ============================================================================
// Re-exports：保持 `use crate::application::selectors::xxx` 路径不变
// ============================================================================

#[cfg(any(target_os = "linux", test))]
pub use dbus_dto::{
    format_connection_status, format_provider_id, format_status_level, DBusHeaderInfo,
    DBusProviderEntry, DBusQuotaEntry, DBusQuotaSnapshot,
};
pub use debug::{
    build_debug_info_text, debug_tab_view_state, DebugContext, DebugTabViewState, LogLevelColor,
};
pub(crate) use format::format_quota_label;
pub use format::quota_usage_detail_text;
#[allow(unused_imports)]
pub use issue_report::{build_issue_report, build_issue_url, IssueReportContext};
pub use settings::settings_providers_tab_view_state;
pub(crate) use tray::compact_quota_display_text;
pub use tray::{
    header_view_state, overview_view_state, provider_detail_view_state,
    tray_global_actions_view_state,
};
