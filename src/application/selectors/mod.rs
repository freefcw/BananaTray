mod debug;
#[allow(dead_code)] // 函数已就绪，待后续 UI 接入后启用
mod format;
mod settings;
mod tray;

// ============================================================================
// ViewModel 类型定义（所有 selector 的共享产出物）
// ============================================================================

use crate::app_state::HeaderStatusKind;
use crate::models::{ProviderKind, QuotaInfo};

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
    pub kind: Option<ProviderKind>,
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
    pub kind: ProviderKind,
    pub icon: String,
    pub title: String,
    pub hint: String,
}

#[derive(Debug, Clone)]
pub struct ProviderPanelViewState {
    pub kind: ProviderKind,
    pub show_dashboard: bool,
    pub body: ProviderBodyViewState,
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
    pub kind: ProviderKind,
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
}

#[derive(Debug, Clone)]
pub struct SettingsProviderListItemViewState {
    pub kind: ProviderKind,
    pub icon: String,
    pub display_name: String,
    pub is_selected: bool,
    pub is_enabled: bool,
    pub can_move_up: bool,
    pub can_move_down: bool,
}

#[derive(Debug, Clone)]
pub struct SettingsProviderDetailViewState {
    pub kind: ProviderKind,
    pub icon: String,
    pub display_name: String,
    pub subtitle: String,
    pub is_enabled: bool,
    pub info: SettingsProviderInfoViewState,
    pub usage: SettingsProviderUsageViewState,
    pub settings_mode: ProviderSettingsMode,
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
    format_amount, format_quota_usage, provider_account_label, provider_list_subtitle,
};
pub use settings::settings_providers_tab_view_state;
pub use tray::{header_view_state, provider_detail_view_state, tray_global_actions_view_state};
