mod action;
pub(crate) mod effect;
#[cfg(any(feature = "app", test))]
pub(crate) mod newapi_ops;
mod quota_alert;
mod reducer;
mod selectors;
pub(crate) mod state;

pub use action::{AppAction, DebugNotificationKind, SettingChange};
pub use effect::{
    AppEffect, CommonEffect, ContextEffect, DebugEffect, NewApiEffect, NotificationEffect,
    RefreshEffect, SettingsEffect, TrayIconRequest,
};
pub use quota_alert::QuotaAlert;
pub use reducer::{build_config_sync_request, reduce};
pub use selectors::{
    build_debug_info_text, build_issue_report, build_issue_url, debug_tab_view_state,
    header_view_state, overview_view_state, provider_detail_view_state, quota_usage_detail_text,
    settings_providers_tab_view_state, tray_global_actions_view_state, AccountInfoViewState,
    AvailableProviderItem, DebugContext, DebugTabViewState, DisabledProviderViewState,
    IssueReportContext, LogLevelColor, OverviewItemStatus, OverviewItemViewState,
    OverviewQuotaItem, ProviderBodyViewState, ProviderDetailViewState, ProviderEmptyAction,
    ProviderEmptyViewState, ProviderPanelViewState, QuotaDisplayViewState, QuotaVisibilityItem,
    RefreshTarget, SettingsProviderDetailViewState, SettingsProviderInfoViewState,
    SettingsProviderListItemViewState, SettingsProviderStatusKind, SettingsProviderUsageViewState,
};
#[cfg_attr(not(target_os = "linux"), allow(unused_imports))]
pub(crate) use selectors::{compact_quota_display_text, format_quota_label};
#[cfg(any(target_os = "linux", test))]
pub use selectors::{
    format_connection_status, format_provider_id, format_status_level, DBusHeaderInfo,
    DBusProviderEntry, DBusQuotaEntry, DBusQuotaSnapshot,
};
pub use state::{AppSession, GlobalHotkeyError, HeaderStatusKind, SettingsTab};
