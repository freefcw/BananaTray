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
    OpenLogDirectory,
    CopyToClipboard(String),
    QuitApp,
}

#[derive(Debug)]
pub enum CommonEffect {
    PersistSettings,
    SendRefreshRequest(RefreshRequest),
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
    OpenLogDirectory,
    CopyToClipboard(String),
}

#[derive(Debug)]
pub enum RoutedEffect {
    Common(CommonEffect),
    Render,
    OpenSettingsWindow,
    OpenUrl(String),
    QuitApp,
}

pub fn route_effect(effect: AppEffect) -> RoutedEffect {
    match effect {
        AppEffect::Render => RoutedEffect::Render,
        AppEffect::PersistSettings => RoutedEffect::Common(CommonEffect::PersistSettings),
        AppEffect::SendRefreshRequest(request) => {
            RoutedEffect::Common(CommonEffect::SendRefreshRequest(request))
        }
        AppEffect::OpenSettingsWindow => RoutedEffect::OpenSettingsWindow,
        AppEffect::OpenUrl(url) => RoutedEffect::OpenUrl(url),
        AppEffect::SyncAutoLaunch(enabled) => {
            RoutedEffect::Common(CommonEffect::SyncAutoLaunch(enabled))
        }
        AppEffect::ApplyLocale(language) => {
            RoutedEffect::Common(CommonEffect::ApplyLocale(language))
        }
        AppEffect::UpdateLogLevel(level) => {
            RoutedEffect::Common(CommonEffect::UpdateLogLevel(level))
        }
        AppEffect::SendQuotaNotification { alert, with_sound } => {
            RoutedEffect::Common(CommonEffect::SendQuotaNotification { alert, with_sound })
        }
        AppEffect::SendDebugNotification { kind, with_sound } => {
            RoutedEffect::Common(CommonEffect::SendDebugNotification { kind, with_sound })
        }
        AppEffect::OpenLogDirectory => RoutedEffect::Common(CommonEffect::OpenLogDirectory),
        AppEffect::CopyToClipboard(text) => {
            RoutedEffect::Common(CommonEffect::CopyToClipboard(text))
        }
        AppEffect::QuitApp => RoutedEffect::QuitApp,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_effect_maps_common_variants() {
        assert!(matches!(
            route_effect(AppEffect::PersistSettings),
            RoutedEffect::Common(CommonEffect::PersistSettings)
        ));
        assert!(matches!(
            route_effect(AppEffect::SendRefreshRequest(RefreshRequest::Shutdown)),
            RoutedEffect::Common(CommonEffect::SendRefreshRequest(RefreshRequest::Shutdown))
        ));
        assert!(matches!(
            route_effect(AppEffect::SyncAutoLaunch(true)),
            RoutedEffect::Common(CommonEffect::SyncAutoLaunch(true))
        ));
        assert!(matches!(
            route_effect(AppEffect::ApplyLocale("zh-CN".to_string())),
            RoutedEffect::Common(CommonEffect::ApplyLocale(language)) if language == "zh-CN"
        ));
        assert!(matches!(
            route_effect(AppEffect::UpdateLogLevel("debug".to_string())),
            RoutedEffect::Common(CommonEffect::UpdateLogLevel(level)) if level == "debug"
        ));
        assert!(matches!(
            route_effect(AppEffect::OpenLogDirectory),
            RoutedEffect::Common(CommonEffect::OpenLogDirectory)
        ));
        assert!(matches!(
            route_effect(AppEffect::CopyToClipboard("hello".to_string())),
            RoutedEffect::Common(CommonEffect::CopyToClipboard(text)) if text == "hello"
        ));
    }

    #[test]
    fn route_effect_preserves_notification_payloads() {
        assert!(matches!(
            route_effect(AppEffect::SendQuotaNotification {
                alert: QuotaAlert::LowQuota {
                    provider_name: "Claude".to_string(),
                    remaining_pct: 8.0,
                },
                with_sound: true,
            }),
            RoutedEffect::Common(CommonEffect::SendQuotaNotification {
                alert: QuotaAlert::LowQuota {
                    provider_name,
                    remaining_pct,
                },
                with_sound: true,
            }) if provider_name == "Claude" && remaining_pct == 8.0
        ));
        assert!(matches!(
            route_effect(AppEffect::SendDebugNotification {
                kind: DebugNotificationKind::Recovered,
                with_sound: false,
            }),
            RoutedEffect::Common(CommonEffect::SendDebugNotification {
                kind: DebugNotificationKind::Recovered,
                with_sound: false,
            })
        ));
    }

    #[test]
    fn route_effect_keeps_runtime_specific_variants() {
        assert!(matches!(
            route_effect(AppEffect::Render),
            RoutedEffect::Render
        ));
        assert!(matches!(
            route_effect(AppEffect::OpenSettingsWindow),
            RoutedEffect::OpenSettingsWindow
        ));
        assert!(matches!(
            route_effect(AppEffect::OpenUrl("https://example.com".to_string())),
            RoutedEffect::OpenUrl(url) if url == "https://example.com"
        ));
        assert!(matches!(
            route_effect(AppEffect::QuitApp),
            RoutedEffect::QuitApp
        ));
    }
}
