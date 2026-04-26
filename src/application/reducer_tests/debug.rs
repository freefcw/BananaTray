use super::common::{has_effect, has_render, make_session, make_session_without, pid};
use crate::application::{
    reduce, AppAction, AppEffect, CommonEffect, ContextEffect, DebugEffect, RefreshEffect,
};
use crate::models::{NavTab, ProviderKind};
use crate::refresh::{RefreshEvent, RefreshOutcome, RefreshRequest, RefreshResult};

// ── SelectDebugProvider ─────────────────────────────

#[test]
fn select_debug_provider_updates_state() {
    let mut session = make_session();
    assert!(session.debug_ui.selected_provider.is_none());

    let effects = reduce(
        &mut session,
        AppAction::SelectDebugProvider(pid(ProviderKind::Claude)),
    );

    assert_eq!(
        session.debug_ui.selected_provider,
        Some(pid(ProviderKind::Claude))
    );
    assert!(has_render(&effects));
}

#[test]
fn select_debug_provider_can_change() {
    let mut session = make_session();
    reduce(
        &mut session,
        AppAction::SelectDebugProvider(pid(ProviderKind::Claude)),
    );
    reduce(
        &mut session,
        AppAction::SelectDebugProvider(pid(ProviderKind::Copilot)),
    );

    assert_eq!(
        session.debug_ui.selected_provider,
        Some(pid(ProviderKind::Copilot))
    );
}

#[test]
fn open_url_produces_context_effect() {
    let mut session = make_session();

    let effects = reduce(
        &mut session,
        AppAction::OpenUrl("https://example.com".to_string()),
    );

    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Context(ContextEffect::OpenUrl(url)) if url == "https://example.com"
    )));
}

#[test]
fn quit_app_produces_context_effect() {
    let mut session = make_session();

    let effects = reduce(&mut session, AppAction::QuitApp);

    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Context(ContextEffect::QuitApp)
    )));
}

// ── DebugRefreshProvider ────────────────────────────

#[test]
fn debug_refresh_without_selection_is_noop() {
    let mut session = make_session();
    let effects = reduce(&mut session, AppAction::DebugRefreshProvider);

    assert!(!session.debug_ui.refresh_active);
    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Debug(DebugEffect::StartRefresh(_)))
    )));
}

#[test]
fn debug_refresh_with_selection_produces_effect() {
    let mut session = make_session();
    reduce(
        &mut session,
        AppAction::SelectDebugProvider(pid(ProviderKind::Gemini)),
    );

    let effects = reduce(&mut session, AppAction::DebugRefreshProvider);

    assert!(session.debug_ui.refresh_active);
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Debug(DebugEffect::StartRefresh(_)))
    )));
}

#[test]
fn debug_refresh_non_monitorable_provider_is_noop() {
    let mut session = make_session();
    reduce(
        &mut session,
        AppAction::SelectDebugProvider(pid(ProviderKind::Kilo)),
    );

    let effects = reduce(&mut session, AppAction::DebugRefreshProvider);

    assert!(!session.debug_ui.refresh_active);
    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Debug(DebugEffect::StartRefresh(_)))
    )));
}

#[test]
fn toggle_non_monitorable_provider_on_renders_without_refresh_request() {
    let mut session = make_session();
    let id = pid(ProviderKind::Kilo);

    let effects = reduce(&mut session, AppAction::ToggleProvider(id.clone()));

    assert!(session.settings.provider.is_enabled(&id));
    assert_eq!(session.nav.active_tab, NavTab::Provider(id));
    assert!(has_render(&effects));
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Refresh(RefreshEffect::SendRequest(
            RefreshRequest::UpdateConfig { .. }
        )))
    )));
    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Refresh(RefreshEffect::SendRequest(
            RefreshRequest::RefreshOne { .. }
        )))
    )));
}

#[test]
fn debug_refresh_while_active_is_noop() {
    let mut session = make_session();
    reduce(
        &mut session,
        AppAction::SelectDebugProvider(pid(ProviderKind::Gemini)),
    );
    reduce(&mut session, AppAction::DebugRefreshProvider);

    // 再次点击不应重复触发
    let effects = reduce(&mut session, AppAction::DebugRefreshProvider);
    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Debug(DebugEffect::StartRefresh(_)))
    )));
}

// ── ClearDebugLogs ──────────────────────────────────

#[test]
fn clear_debug_logs_produces_effect() {
    let mut session = make_session();
    let effects = reduce(&mut session, AppAction::ClearDebugLogs);

    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Debug(DebugEffect::ClearLogs))
    )));
    assert!(has_render(&effects));
}
// ── RefreshEvent::Finished + debug restore ──────────

#[test]
fn finished_event_restores_debug_state() {
    let mut session = make_session();
    let id = pid(ProviderKind::Claude);

    session.debug_ui.selected_provider = Some(id.clone());
    session.debug_ui.refresh_active = true;
    session.debug_ui.prev_log_level = Some(log::LevelFilter::Info);

    let outcome = RefreshOutcome {
        id,
        result: RefreshResult::Failed {
            failure: crate::models::ProviderFailure {
                reason: crate::models::FailureReason::FetchFailed,
                advice: None,
                raw_detail: Some("test error".to_string()),
            },
            error_kind: crate::models::ErrorKind::NetworkError,
        },
    };
    let effects = reduce(
        &mut session,
        AppAction::RefreshEventReceived(RefreshEvent::Finished(outcome)),
    );

    assert!(!session.debug_ui.refresh_active);
    assert!(session.debug_ui.prev_log_level.is_none());
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Debug(DebugEffect::RestoreLogLevel(
            log::LevelFilter::Info
        )))
    )));
}

#[test]
fn finished_event_for_other_provider_does_not_restore() {
    let mut session = make_session();

    session.debug_ui.selected_provider = Some(pid(ProviderKind::Claude));
    session.debug_ui.refresh_active = true;
    session.debug_ui.prev_log_level = Some(log::LevelFilter::Info);

    let outcome = RefreshOutcome {
        id: pid(ProviderKind::Gemini),
        result: RefreshResult::SkippedCooldown,
    };
    let effects = reduce(
        &mut session,
        AppAction::RefreshEventReceived(RefreshEvent::Finished(outcome)),
    );

    assert!(session.debug_ui.refresh_active);
    assert!(session.debug_ui.prev_log_level.is_some());
    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Debug(DebugEffect::RestoreLogLevel(_)))
    )));
}

#[test]
fn finished_restore_survives_unknown_provider() {
    let mut session = make_session_without(ProviderKind::Claude);
    let id = pid(ProviderKind::Claude);

    session.debug_ui.selected_provider = Some(id.clone());
    session.debug_ui.refresh_active = true;
    session.debug_ui.prev_log_level = Some(log::LevelFilter::Warn);

    let outcome = RefreshOutcome {
        id,
        result: RefreshResult::Failed {
            failure: crate::models::ProviderFailure {
                reason: crate::models::FailureReason::FetchFailed,
                advice: None,
                raw_detail: Some("gone".to_string()),
            },
            error_kind: crate::models::ErrorKind::Unknown,
        },
    };
    let effects = reduce(
        &mut session,
        AppAction::RefreshEventReceived(RefreshEvent::Finished(outcome)),
    );

    assert!(!session.debug_ui.refresh_active);
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Debug(DebugEffect::RestoreLogLevel(
            log::LevelFilter::Warn
        )))
    )));
}
