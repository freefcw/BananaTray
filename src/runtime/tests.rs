use super::*;
use crate::application::{AppAction, TrayIconRequest};
use crate::models::{
    AppSettings, ConnectionStatus, CustomProviderLifecycleFailure, ErrorKind, ProviderId,
    ProviderKind, ScriptProviderConfig, TrayIconStyle,
};
use crate::providers::{ProviderManager, ProviderManagerHandle};
use crate::refresh::{RefreshReason, RefreshRequest};
use crate::runtime::{CustomProviderJob, ScriptTestJob};

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
    let (custom_provider_tx, _custom_provider_rx) = PersistentJobSender::channel(1);
    let (script_test_tx, _script_test_rx) = BackgroundJobSender::channel(1);
    let manager = ProviderManagerHandle::new(ProviderManager::new());
    Rc::new(RefCell::new(AppState::new(
        crate::refresh::RefreshWorker::detached(tx),
        custom_provider_tx,
        script_test_tx,
        manager,
        AppSettings::default(),
        None,
    )))
}

fn make_state_with_background_receiver() -> (
    Rc<RefCell<AppState>>,
    PersistentJobReceiver<CustomProviderJob>,
) {
    let (tx, _rx) = smol::channel::bounded(1);
    let (custom_provider_tx, custom_provider_rx) = PersistentJobSender::channel(8);
    let (script_test_tx, _script_test_rx) = BackgroundJobSender::channel(8);
    let manager = ProviderManagerHandle::new(ProviderManager::new());
    (
        Rc::new(RefCell::new(AppState::new(
            crate::refresh::RefreshWorker::detached(tx),
            custom_provider_tx,
            script_test_tx,
            manager,
            AppSettings::default(),
            None,
        ))),
        custom_provider_rx,
    )
}

fn make_state_with_full_refresh_queue() -> Rc<RefCell<AppState>> {
    let (tx, _rx) = smol::channel::bounded(1);
    tx.try_send(RefreshRequest::Shutdown)
        .expect("refresh queue should accept filler request");
    let (custom_provider_tx, _custom_provider_rx) = PersistentJobSender::channel(1);
    let (script_test_tx, _script_test_rx) = BackgroundJobSender::channel(1);
    let manager = ProviderManagerHandle::new(ProviderManager::new());
    Rc::new(RefCell::new(AppState::new(
        crate::refresh::RefreshWorker::detached(tx),
        custom_provider_tx,
        script_test_tx,
        manager,
        AppSettings::default(),
        None,
    )))
}

fn make_state_with_full_script_queue() -> Rc<RefCell<AppState>> {
    let (tx, _rx) = smol::channel::bounded(1);
    let (custom_provider_tx, _custom_provider_rx) = PersistentJobSender::channel(1);
    let (script_test_tx, _script_test_rx) = BackgroundJobSender::channel(1);
    script_test_tx
        .try_send(ScriptTestJob {
            request_id: 99,
            config: ScriptProviderConfig {
                display_name: "Filler".to_string(),
                provider_id: "filler:script".to_string(),
                interpreter: "sh".to_string(),
                timeout_ms: 1_000,
                script: "echo {}".to_string(),
            },
        })
        .expect("script queue should accept filler request");
    let manager = ProviderManagerHandle::new(ProviderManager::new());
    Rc::new(RefCell::new(AppState::new(
        crate::refresh::RefreshWorker::detached(tx),
        custom_provider_tx,
        script_test_tx,
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
fn app_state_shutdown_requests_both_background_workers_to_stop() {
    let state = make_state();
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(50);

    state
        .borrow_mut()
        .shutdown_background_workers_before(deadline);

    assert!(state.borrow().custom_provider_tx.is_shutdown());
    assert!(state.borrow().script_test_tx.is_shutdown());
}

#[test]
fn shutdown_settles_completed_save_failure_before_final_settings_flush() {
    let state = make_state();
    let records = std::sync::Arc::new(std::sync::Mutex::new(Vec::<AppSettings>::new()));
    let recorded = records.clone();
    state.borrow_mut().settings_writer = SettingsWriter::spawn_for_test(move |settings| {
        recorded.lock().unwrap().push(settings.clone());
        true
    });

    let config = crate::models::NewApiConfig {
        display_name: "Exit race".to_string(),
        base_url: "https://exit-race.example.com".to_string(),
        cookie: "session=test".to_string(),
        user_id: None,
        divisor: None,
    };
    let provider_id = ProviderId::Custom(crate::models::newapi_provider_id(
        &config.base_url,
        config.user_id.as_deref(),
    ));
    {
        let mut state = state.borrow_mut();
        state.session.settings_ui.modal = crate::application::SettingsModalState::AddingNewApi;
        let _ =
            crate::application::reduce(&mut state.session, AppAction::SubmitNewApi(config.clone()));
        let request_id = state
            .session
            .settings_ui
            .pending_custom_provider_save_request_id
            .expect("save request id");
        state
            .settings_writer
            .schedule(state.session.settings.clone());
        state
            .custom_provider_results
            .push(AppAction::NewApiSaveFinished {
                request_id,
                config,
                filename: "exit-race.yaml".to_string(),
                original_id: None,
                is_editing: false,
                result: Err(CustomProviderLifecycleFailure::file_operation(
                    "save NewAPI provider",
                    "disk full",
                )),
            });
    }

    state
        .borrow_mut()
        .shutdown_before(std::time::Instant::now() + std::time::Duration::from_millis(50));

    assert!(!state
        .borrow()
        .session
        .settings
        .provider
        .enabled_providers
        .contains_key(&provider_id.id_key()));
    let records = records.lock().unwrap();
    let final_settings = records.last().expect("final settings snapshot");
    assert!(!final_settings
        .provider
        .enabled_providers
        .contains_key(&provider_id.id_key()));
}

#[test]
fn shutdown_settles_completed_delete_success_before_final_settings_flush() {
    let state = make_state();
    let provider_id = ProviderId::Custom("deleted:newapi".to_string());
    {
        let mut state = state.borrow_mut();
        state
            .session
            .settings
            .provider
            .set_enabled(&provider_id, true);
        state.session.settings.provider.add_to_sidebar(&provider_id);
        let request_id = state
            .session
            .settings_ui
            .begin_custom_provider_delete(provider_id.clone());
        state
            .custom_provider_results
            .push(AppAction::NewApiDeleteFinished {
                request_id,
                provider_id: provider_id.clone(),
                result: Ok(std::path::PathBuf::from("deleted.yaml")),
            });
    }

    state
        .borrow_mut()
        .shutdown_before(std::time::Instant::now() + std::time::Duration::from_millis(50));

    let state = state.borrow();
    assert!(!state
        .session
        .settings
        .provider
        .enabled_providers
        .contains_key(&provider_id.id_key()));
    assert!(!state
        .session
        .settings
        .provider
        .sidebar_providers
        .contains(&provider_id.id_key()));
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
fn dispatch_queues_custom_provider_io_instead_of_running_it_inline() {
    let (state, background_rx) = make_state_with_background_receiver();
    let provider_id = ProviderId::Custom("missing:newapi".to_string());
    let mut caps = FakeCaps::default();

    dispatch_with_full_context(
        &state,
        AppAction::EditNewApi {
            provider_id: provider_id.clone(),
        },
        &mut caps,
    );

    let job = background_rx
        .try_recv()
        .expect("custom-provider disk I/O should be queued for the blocking worker");
    assert!(matches!(
        job,
        CustomProviderJob::NewApi {
            effect: crate::application::NewApiEffect::LoadConfig {
                provider_id: queued_id
            },
            ..
        } if queued_id == provider_id
    ));
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
