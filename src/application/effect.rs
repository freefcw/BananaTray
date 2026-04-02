use crate::application::DebugNotificationKind;
use crate::notification::QuotaAlert;
use crate::refresh::RefreshRequest;

#[derive(Debug)]
pub enum AppEffect {
    Render,
    PersistSettings,
    SendRefreshRequest(RefreshRequest),
    OpenSettingsWindow,
    OpenUrl(String),
    SyncAutoLaunch(bool),
    ApplyLocale(String),
    UpdateLogLevel(String),
    SendQuotaNotification {
        alert: QuotaAlert,
        with_sound: bool,
    },
    SendDebugNotification {
        kind: DebugNotificationKind,
        with_sound: bool,
    },
    QuitApp,
}
