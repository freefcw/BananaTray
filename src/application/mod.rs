mod action;
pub(crate) mod effect;
mod reducer;
mod selectors;
pub(crate) mod state;

pub use action::{AppAction, DebugNotificationKind, SettingChange};
pub use effect::{AppEffect, TrayIconRequest};
pub use reducer::{build_config_sync_request, reduce};
#[allow(unused_imports)]
pub use selectors::{
    build_debug_info_text, debug_tab_view_state, header_view_state, overview_view_state,
    provider_detail_view_state, quota_remaining_text, quota_usage_detail_text,
    settings_providers_tab_view_state, tray_global_actions_view_state, AccountInfoViewState,
    AvailableProviderItem, CapturedLogEntry, DebugConsoleViewState, DebugContext,
    DebugTabViewState, DisabledProviderViewState, LogLevelColor, OverviewItemStatus,
    OverviewItemViewState, OverviewViewState, ProviderBodyViewState, ProviderDetailViewState,
    ProviderEmptyAction, ProviderEmptyViewState, ProviderPanelViewState, ProviderSettingsMode,
    QuotaVisibilityItem, SettingsProviderDetailViewState, SettingsProviderInfoViewState,
    SettingsProviderListItemViewState, SettingsProviderStatusKind, SettingsProviderUsageViewState,
};
pub use state::{AppSession, HeaderStatusKind, SettingsTab};
