mod action;
pub(crate) mod effect;
mod quota_alert;
mod reducer;
mod selectors;
pub(crate) mod state;

pub use action::{AppAction, DebugNotificationKind, SettingChange};
pub use effect::{AppEffect, CommonEffect, ContextEffect, TrayIconRequest};
pub use quota_alert::QuotaAlert;
pub use reducer::{build_config_sync_request, reduce};
#[allow(unused_imports)]
pub use selectors::{
    build_debug_info_text, build_issue_report, build_issue_url, debug_tab_view_state,
    header_view_state, overview_view_state, provider_detail_view_state, quota_remaining_text,
    quota_usage_detail_text, settings_providers_tab_view_state, tray_global_actions_view_state,
    AccountInfoViewState, AvailableProviderItem, CapturedLogEntry, DebugConsoleViewState,
    DebugContext, DebugTabViewState, DisabledProviderViewState, IssueReportContext, LogLevelColor,
    OverviewItemStatus, OverviewItemViewState, OverviewQuotaItem, OverviewViewState,
    ProviderBodyViewState, ProviderDetailViewState, ProviderEmptyAction, ProviderEmptyViewState,
    ProviderPanelViewState, QuotaVisibilityItem, RefreshTarget, SettingsProviderDetailViewState,
    SettingsProviderInfoViewState, SettingsProviderListItemViewState, SettingsProviderStatusKind,
    SettingsProviderUsageViewState,
};
pub use state::{AppSession, HeaderStatusKind, SettingsTab};
