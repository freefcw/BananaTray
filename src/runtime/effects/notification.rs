use rust_i18n::t;

use crate::application::{DebugNotificationKind, NotificationEffect, QuotaAlert};
use crate::platform::notification::send_system_notification;

pub(super) fn run(effect: NotificationEffect) {
    match effect {
        NotificationEffect::Quota { alert, with_sound } => {
            send_system_notification(&alert, with_sound);
        }
        NotificationEffect::AutoLaunchToggled { enabled } => {
            let (title, body) = if enabled {
                (
                    t!("notification.auto_launch.enabled.title").to_string(),
                    t!("notification.auto_launch.enabled.body").to_string(),
                )
            } else {
                (
                    t!("notification.auto_launch.disabled.title").to_string(),
                    t!("notification.auto_launch.disabled.body").to_string(),
                )
            };
            crate::platform::notification::send_plain_notification(&title, &body);
        }
        NotificationEffect::Debug { kind, with_sound } => {
            send_system_notification(&build_debug_alert(kind), with_sound);
        }
    }
}

fn build_debug_alert(kind: DebugNotificationKind) -> QuotaAlert {
    match kind {
        DebugNotificationKind::Low => QuotaAlert::LowQuota {
            provider_name: "TestProvider".to_string(),
            remaining_pct: 8.0,
        },
        DebugNotificationKind::Exhausted => QuotaAlert::Exhausted {
            provider_name: "TestProvider".to_string(),
        },
        DebugNotificationKind::Recovered => QuotaAlert::Recovered {
            provider_name: "TestProvider".to_string(),
            remaining_pct: 50.0,
        },
    }
}
