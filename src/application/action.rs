use crate::app_state::SettingsTab;
use crate::models::{AppTheme, NavTab, ProviderKind};
use crate::refresh::{RefreshEvent, RefreshReason};

#[derive(Debug)]
pub enum AppAction {
    SelectNavTab(NavTab),
    SetSettingsTab(SettingsTab),
    SelectSettingsProvider(ProviderKind),
    ToggleCadenceDropdown,
    SetCopilotTokenEditing(bool),
    SaveCopilotToken(String),
    ReorderProvider {
        kind: ProviderKind,
        direction: ProviderOrderDirection,
    },
    UpdateSetting(SettingChange),
    RefreshProvider {
        kind: ProviderKind,
        reason: RefreshReason,
    },
    ToggleProvider(ProviderKind),
    RefreshEventReceived(RefreshEvent),
    OpenSettings {
        provider: Option<ProviderKind>,
    },
    OpenDashboard(ProviderKind),
    OpenUrl(String),
    UpdateLogLevel(String),
    SendDebugNotification(DebugNotificationKind),
    OpenLogDirectory,
    CopyToClipboard(String),
    QuitApp,
}

#[derive(Debug, Clone, Copy)]
pub enum ProviderOrderDirection {
    Up,
    Down,
}

#[derive(Debug, Clone)]
pub enum SettingChange {
    ToggleAutoHideWindow,
    ToggleStartAtLogin,
    ToggleSessionQuotaNotifications,
    ToggleNotificationSound,
    ToggleShowDashboardButton,
    ToggleShowRefreshButton,
    ToggleShowDebugTab,
    Theme(AppTheme),
    Language(String),
    RefreshCadence(Option<u64>),
}

#[derive(Debug, Clone, Copy)]
pub enum DebugNotificationKind {
    Low,
    Exhausted,
    Recovered,
}
