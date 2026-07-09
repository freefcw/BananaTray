mod action;
pub(crate) mod effect;
pub(crate) mod newapi_ops;
mod quota_alert;
mod reducer;
pub(crate) mod script_provider_ops;
mod selectors;
pub(crate) mod state;

pub use action::{AppAction, DebugNotificationKind, SettingChange};
pub use effect::{
    AppEffect, CommonEffect, ContextEffect, DebugEffect, NewApiEffect, NotificationEffect,
    RefreshEffect, ScriptProviderEffect, SettingsEffect, TrayIconRequest,
};
pub use quota_alert::QuotaAlert;
pub use reducer::{build_config_sync_request, reduce};
pub use selectors::{
    build_debug_info_text, build_issue_report, build_issue_url, debug_tab_view_state,
    format_debug_console_logs, header_view_state, overview_view_state, provider_detail_view_state,
    settings_providers_tab_view_state, tray_global_actions_view_state, AccountInfoViewState,
    AvailableProviderItem, DebugContext, DebugTabViewState, DisabledProviderViewState,
    EnvironmentRowKind, IssueReportContext, LogLevelColor, OverviewItemStatus,
    OverviewItemViewState, OverviewQuotaItem, ProviderBodyViewState, ProviderDetailViewState,
    ProviderEmptyAction, ProviderEmptyViewState, ProviderPanelViewState, QuotaDisplayViewState,
    QuotaVisibilityItem, RefreshTarget, SettingsProviderDetailViewState,
    SettingsProviderInfoViewState, SettingsProviderListItemViewState,
    SettingsProviderRightPaneViewState, SettingsProviderStatusKind, SettingsProviderUsageViewState,
};
#[allow(unused_imports)] // app feature 下 ui/widgets 使用
pub(crate) use selectors::{
    format_quota_card_detail_text, format_quota_card_display_text, format_quota_card_has_unit,
    format_quota_card_mode_label,
};
#[cfg(any(target_os = "linux", test))]
#[allow(unused_imports)]
pub use selectors::{DBusHeaderInfo, DBusProviderEntry, DBusQuotaEntry, DBusQuotaSnapshot};
#[allow(unused_imports)] // app/UI 和 reducer 测试通过 application facade 使用这些状态类型
pub use state::{
    AppSession, FormIdentity, GlobalHotkeyError, HeaderStatusKind, SettingsModalState, SettingsTab,
};
