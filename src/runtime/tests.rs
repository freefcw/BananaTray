use super::*;
use crate::application::{AppAction, TrayIconRequest};
use crate::models::{
    AppSettings, ConnectionStatus, ErrorKind, ProviderId, ProviderKind, ScriptProviderConfig,
    TrayIconStyle,
};
use crate::providers::ProviderManagerHandle;
use crate::refresh::{RefreshReason, RefreshRequest};

#[derive(Default)]
struct FakeCaps {
    rendered: bool,
    settings_opened: bool,
    tray_icon_applied: bool,
    hotkey_applied: bool,
    quit: bool,
}

impl ContextCapabilities for FakeCaps {
    fn render(&mut self, _state: &Rc<RefCell<AppState>>) {
        self.rendered = true;
    }
}

impl FullContextCapabilities for FakeCaps {
    fn open_settings_window(&mut self, _state: &Rc<RefCell<AppState>>) {
        self.settings_opened = true;
    }

    fn apply_tray_icon(&mut self, _request: TrayIconRequest) {
        self.tray_icon_applied = true;
    }

    fn apply_global_hotkey(&mut self, _state: &Rc<RefCell<AppState>>, hotkey: &str) -> AppAction {
        self.hotkey_applied = true;
        AppAction::GlobalHotkeyApplyFinished {
            requested: hotkey.to_string(),
            result: Ok(hotkey.to_string()),
        }
    }

    fn quit(&mut self) {
        self.quit = true;
    }
}

fn make_state() -> Rc<RefCell<AppState>> {
    let (tx, _rx) = smol::channel::bounded(1);
    let (script_tx, _script_rx) = smol::channel::bounded(1);
    let manager = ProviderManagerHandle::default();
    Rc::new(RefCell::new(AppState::new(
        crate::refresh::RefreshWorker::detached(tx),
        script_tx,
        manager,
        AppSettings::default(),
        None,
    )))
}

fn make_state_with_full_refresh_queue() -> Rc<RefCell<AppState>> {
    let (tx, _rx) = smol::channel::bounded(1);
    tx.try_send(RefreshRequest::Shutdown)
        .expect("refresh queue should accept filler request");
    let (script_tx, _script_rx) = smol::channel::bounded(1);
    let manager = ProviderManagerHandle::default();
    Rc::new(RefCell::new(AppState::new(
        crate::refresh::RefreshWorker::detached(tx),
        script_tx,
        manager,
        AppSettings::default(),
        None,
    )))
}

fn make_state_with_full_script_queue() -> Rc<RefCell<AppState>> {
    let (tx, _rx) = smol::channel::bounded(1);
    let (script_tx, _script_rx) = smol::channel::bounded(1);
    script_tx
        .try_send((
            99,
            ScriptProviderConfig {
                display_name: "Filler".to_string(),
                provider_id: "filler:script".to_string(),
                interpreter: "sh".to_string(),
                timeout_ms: 1_000,
                script: "echo {}".to_string(),
            },
        ))
        .expect("script queue should accept filler request");
    let manager = ProviderManagerHandle::default();
    Rc::new(RefCell::new(AppState::new(
        crate::refresh::RefreshWorker::detached(tx),
        script_tx,
        manager,
        AppSettings::default(),
        None,
    )))
}

#[test]
fn run_context_effect_routes_render_to_capability() {
    let state = make_state();
    let mut caps = FakeCaps::default();

    let _ = run_full_context_effect(&state, ContextEffect::Render, &mut caps);

    assert!(caps.rendered);
}

#[test]
fn app_state_shutdown_stops_settings_writer() {
    let state = make_state();

    assert!(!state.borrow().settings_writer.is_shutdown());
    state.borrow_mut().shutdown_settings_writer();

    assert!(state.borrow().settings_writer.is_shutdown());
}

#[test]
fn run_context_effect_routes_full_context_capabilities() {
    let state = make_state();
    let mut caps = FakeCaps::default();

    let _ = run_full_context_effect(&state, ContextEffect::OpenSettingsWindow, &mut caps);
    let _ = run_full_context_effect(
        &state,
        ContextEffect::ApplyTrayIcon(TrayIconRequest::Static(TrayIconStyle::Yellow)),
        &mut caps,
    );
    let actions = run_full_context_effect(
        &state,
        ContextEffect::ApplyGlobalHotkey("Cmd+Shift+B".to_string()),
        &mut caps,
    );
    let _ = run_full_context_effect(&state, ContextEffect::QuitApp, &mut caps);

    assert!(matches!(
        actions.as_slice(),
        [AppAction::GlobalHotkeyApplyFinished { result: Ok(value), .. }] if value == "Cmd+Shift+B"
    ));

    assert!(caps.settings_opened);
    assert!(caps.tray_icon_applied);
    assert!(caps.hotkey_applied);
    assert!(caps.quit);
}

#[test]
#[should_panic(expected = "requires App or Window context")]
fn run_view_context_effect_rejects_open_settings_window() {
    let state = make_state();
    let mut caps = FakeCaps::default();

    run_view_context_effect(&state, ContextEffect::OpenSettingsWindow, &mut caps);
}

#[test]
#[should_panic(expected = "requires App or Window context")]
fn run_view_context_effect_rejects_apply_tray_icon() {
    let state = make_state();
    let mut caps = FakeCaps::default();

    run_view_context_effect(
        &state,
        ContextEffect::ApplyTrayIcon(TrayIconRequest::Static(TrayIconStyle::Yellow)),
        &mut caps,
    );
}

#[test]
#[should_panic(expected = "requires App or Window context")]
fn run_view_context_effect_rejects_apply_global_hotkey() {
    let state = make_state();
    let mut caps = FakeCaps::default();

    run_view_context_effect(
        &state,
        ContextEffect::ApplyGlobalHotkey("Cmd+Shift+B".to_string()),
        &mut caps,
    );
}

#[test]
#[should_panic(expected = "requires App or Window context")]
fn run_view_context_effect_rejects_quit_app() {
    let state = make_state();
    let mut caps = FakeCaps::default();

    run_view_context_effect(&state, ContextEffect::QuitApp, &mut caps);
}

#[test]
fn dispatch_processes_refresh_send_failure_follow_up_action() {
    let state = make_state_with_full_refresh_queue();
    let provider_id = ProviderId::BuiltIn(ProviderKind::Claude);
    state
        .borrow_mut()
        .session
        .settings
        .provider
        .set_enabled(&provider_id, true);
    let mut caps = FakeCaps::default();

    dispatch_with_full_context(
        &state,
        AppAction::RefreshProvider {
            id: provider_id.clone(),
            reason: RefreshReason::Manual,
        },
        &mut caps,
    );

    let state_ref = state.borrow();
    let provider = state_ref
        .session
        .provider_store
        .find_by_id(&provider_id)
        .expect("provider status");
    assert_eq!(provider.connection, ConnectionStatus::Error);
    assert_eq!(provider.error_kind, ErrorKind::Unknown);
    assert!(provider
        .last_failure
        .as_ref()
        .and_then(|failure| failure.raw_detail.as_deref())
        .is_some_and(|detail| detail.contains("refresh coordinator unavailable")));
    assert!(caps.rendered);
}

#[test]
fn dispatch_processes_refresh_all_send_failure_for_every_target() {
    let state = make_state_with_full_refresh_queue();
    let provider_ids = [
        ProviderId::BuiltIn(ProviderKind::Claude),
        ProviderId::BuiltIn(ProviderKind::Gemini),
    ];
    {
        let mut state_ref = state.borrow_mut();
        for id in &provider_ids {
            state_ref.session.settings.provider.set_enabled(id, true);
        }
    }
    let mut caps = FakeCaps::default();

    dispatch_with_full_context(&state, AppAction::RefreshAll, &mut caps);

    let state_ref = state.borrow();
    for id in &provider_ids {
        let provider = state_ref
            .session
            .provider_store
            .find_by_id(id)
            .expect("provider status");
        assert_eq!(provider.connection, ConnectionStatus::Error);
        assert!(provider
            .last_failure
            .as_ref()
            .and_then(|failure| failure.raw_detail.as_deref())
            .is_some_and(|detail| detail.contains("refresh coordinator unavailable")));
    }
    assert!(caps.rendered);
}

#[test]
fn dispatch_processes_debug_refresh_send_failure_follow_up_action() {
    let state = make_state_with_full_refresh_queue();
    let provider_id = ProviderId::BuiltIn(ProviderKind::Claude);
    {
        let mut state_ref = state.borrow_mut();
        state_ref
            .session
            .settings
            .provider
            .set_enabled(&provider_id, true);
        state_ref.session.debug_ui.selected_provider = Some(provider_id.clone());
    }
    let mut caps = FakeCaps::default();

    dispatch_with_full_context(&state, AppAction::DebugRefreshProvider, &mut caps);

    let state_ref = state.borrow();
    let provider = state_ref
        .session
        .provider_store
        .find_by_id(&provider_id)
        .expect("provider status");
    assert_eq!(provider.connection, ConnectionStatus::Error);
    assert!(!state_ref.session.debug_ui.refresh_active);
    assert!(state_ref.session.debug_ui.prev_log_level.is_none());
    assert!(caps.rendered);
}

#[test]
fn dispatch_processes_script_test_queue_failure_follow_up_action() {
    let state = make_state_with_full_script_queue();
    let mut caps = FakeCaps::default();

    dispatch_with_full_context(
        &state,
        AppAction::TestScriptProvider(ScriptProviderConfig {
            display_name: "Script".to_string(),
            provider_id: "script:script".to_string(),
            interpreter: "sh".to_string(),
            timeout_ms: 1_000,
            script: "echo {}".to_string(),
        }),
        &mut caps,
    );

    let state_ref = state.borrow();
    assert!(!state_ref.session.settings_ui.script_provider_testing);
    assert!(state_ref
        .session
        .settings_ui
        .script_provider_pending_test_request_id
        .is_none());
    let result = state_ref
        .session
        .settings_ui
        .script_provider_test_result
        .as_ref()
        .expect("script test result");
    assert!(!result.success);
    assert!(result.message.contains("failed to queue script test"));
    assert!(caps.rendered);
}

#[test]
fn execute_script_provider_test_delegates_to_script_effect_runner() {
    let result = execute_script_provider_test(&ScriptProviderConfig {
        display_name: "Script".to_string(),
        provider_id: "script:script".to_string(),
        interpreter: "sh".to_string(),
        timeout_ms: 1_000,
        script: r#"printf '{"remaining":7,"unit":"USD"}'"#.to_string(),
    });

    assert!(result.success, "unexpected failure: {}", result.message);
    assert_eq!(result.preview.expect("preview").remaining, 7.0);
}
