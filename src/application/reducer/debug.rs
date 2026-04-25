use crate::application::{AppEffect, CommonEffect, ContextEffect, DebugNotificationKind};
use crate::models::ProviderId;

use super::super::state::AppSession;
use super::shared::provider_supports_refresh;

pub(super) fn update_log_level(level: String, effects: &mut Vec<AppEffect>) {
    effects.push(CommonEffect::UpdateLogLevel(level).into());
    effects.push(ContextEffect::Render.into());
}

pub(super) fn send_debug_notification(
    session: &AppSession,
    kind: DebugNotificationKind,
    effects: &mut Vec<AppEffect>,
) {
    effects.push(
        CommonEffect::SendDebugNotification {
            kind,
            with_sound: session.settings.notification.notification_sound,
        }
        .into(),
    );
}

pub(super) fn open_log_directory(effects: &mut Vec<AppEffect>) {
    effects.push(CommonEffect::OpenLogDirectory.into());
}

pub(super) fn copy_to_clipboard(text: String, effects: &mut Vec<AppEffect>) {
    effects.push(CommonEffect::CopyToClipboard(text).into());
}

pub(super) fn select_debug_provider(
    session: &mut AppSession,
    id: ProviderId,
    effects: &mut Vec<AppEffect>,
) {
    session.debug_ui.selected_provider = Some(id);
    effects.push(ContextEffect::Render.into());
}

pub(super) fn debug_refresh_provider(session: &mut AppSession, effects: &mut Vec<AppEffect>) {
    let Some(id) = session.debug_ui.selected_provider.clone() else {
        return;
    };

    let supports_refresh = provider_supports_refresh(session, &id);
    if !session.debug_ui.refresh_active && supports_refresh {
        session.debug_ui.refresh_active = true;
        session.provider_store.mark_refreshing_by_id(&id);
        effects.push(CommonEffect::StartDebugRefresh(id).into());
        effects.push(ContextEffect::Render.into());
    }
}

pub(super) fn clear_debug_logs(effects: &mut Vec<AppEffect>) {
    effects.push(CommonEffect::ClearDebugLogs.into());
    effects.push(ContextEffect::Render.into());
}
