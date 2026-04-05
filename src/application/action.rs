use crate::app_state::SettingsTab;
use crate::models::{AppTheme, NavTab, ProviderId, ProviderKind, QuotaDisplayMode, TrayIconStyle};
use crate::refresh::{RefreshEvent, RefreshReason};

#[derive(Debug)]
pub enum AppAction {
    SelectNavTab(NavTab),
    SetSettingsTab(SettingsTab),
    SelectSettingsProvider(ProviderId),
    ToggleCadenceDropdown,
    SetCopilotTokenEditing(bool),
    SaveCopilotToken(String),
    ReorderProvider {
        id: ProviderId,
        direction: ProviderOrderDirection,
    },
    UpdateSetting(SettingChange),
    RefreshProvider {
        id: ProviderId,
        reason: RefreshReason,
    },
    ToggleProvider(ProviderId),
    RefreshEventReceived(RefreshEvent),
    OpenSettings {
        provider: Option<ProviderId>,
    },
    OpenDashboard(ProviderId),
    OpenUrl(String),
    UpdateLogLevel(String),
    SendDebugNotification(DebugNotificationKind),
    OpenLogDirectory,
    CopyToClipboard(String),
    /// Debug Tab: 选择调试目标 Provider
    SelectDebugProvider(ProviderId),
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
