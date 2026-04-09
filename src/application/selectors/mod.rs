mod debug;
#[allow(dead_code)] // 函数已就绪，待后续 UI 接入后启用
mod format;
mod settings;
mod tray;

// ============================================================================
// ViewModel 类型定义（所有 selector 的共享产出物）
// ============================================================================

use crate::app_state::HeaderStatusKind;
use crate::models::{ProviderId, QuotaDisplayMode, QuotaInfo};
use crate::providers::custom::generator::NewApiEditData;

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

#[derive(Debug, Clone)]
pub struct RefreshButtonViewState {
    pub id: Option<ProviderId>,
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
        quotas: Vec<QuotaInfo>,
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
    pub info: SettingsProviderInfoViewState,
    pub usage: SettingsProviderUsageViewState,
    pub settings_mode: ProviderSettingsMode,
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
    Quotas { quotas: Vec<QuotaInfo> },
    Error { title: String, message: String },
    Empty { message: String },
    Missing { message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderSettingsMode {
    AutoManaged,
    Interactive,
    /// NewAPI 型自定义 Provider — 显示「编辑配置」按钮
    NewApiEditable,
}

// ============================================================================
// Re-exports：保持 `use crate::application::selectors::xxx` 路径不变
// ============================================================================

pub use debug::{
    build_debug_info_text, debug_tab_view_state, CapturedLogEntry, DebugConsoleViewState,
    DebugContext, DebugTabViewState, LogLevelColor,
};
#[allow(unused_imports)] // 函数已就绪，待后续 UI 接入后启用
pub use format::{
    format_amount, format_last_updated, format_quota_usage, provider_account_label,
    provider_list_subtitle, quota_remaining_text, quota_usage_detail_text,
};
pub use settings::settings_providers_tab_view_state;
pub use tray::{header_view_state, provider_detail_view_state, tray_global_actions_view_state};
