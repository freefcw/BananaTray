use crate::app_state::SettingsTab;
use crate::models::{AppTheme, NavTab, ProviderKind, QuotaDisplayMode, TrayIconStyle};
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
    /// Debug Tab: 选择调试目标 Provider
    SelectDebugProvider(ProviderKind),
    /// Debug Tab: 强制刷新选中的 Provider（跳过 cooldown，临时提升日志级别）
    DebugRefreshProvider,
    /// Debug Tab: 清空日志缓冲区
    ClearDebugLogs,
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
    ToggleShowAccountInfo,
    Theme(AppTheme),
    Language(String),
    RefreshCadence(Option<u64>),
    SetTrayIconStyle(TrayIconStyle),
    SetQuotaDisplayMode(QuotaDisplayMode),
    ToggleQuotaVisibility {
        kind: ProviderKind,
        quota_key: String,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum DebugNotificationKind {
    Low,
    Exhausted,
    Recovered,
}
