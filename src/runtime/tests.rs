use super::*;
use crate::application::TrayIconRequest;
use crate::models::{AppSettings, TrayIconStyle};
use crate::providers::ProviderManagerHandle;

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

    fn apply_global_hotkey(&mut self, _state: &Rc<RefCell<AppState>>, _hotkey: &str) {
        self.hotkey_applied = true;
    }

    fn quit(&mut self) {
        self.quit = true;
    }
}

fn make_state() -> Rc<RefCell<AppState>> {
    let (tx, _rx) = smol::channel::bounded(1);
    let manager = ProviderManagerHandle::default();
    Rc::new(RefCell::new(AppState::new(
        tx,
        manager,
        AppSettings::default(),
        None,
    )))
}

#[test]
fn run_context_effect_routes_render_to_capability() {
    let state = make_state();
    let mut caps = FakeCaps::default();

    run_full_context_effect(&state, ContextEffect::Render, &mut caps);

    assert!(caps.rendered);
}

#[test]
fn run_context_effect_routes_full_context_capabilities() {
    let state = make_state();
    let mut caps = FakeCaps::default();

    run_full_context_effect(&state, ContextEffect::OpenSettingsWindow, &mut caps);
    run_full_context_effect(
        &state,
        ContextEffect::ApplyTrayIcon(TrayIconRequest::Static(TrayIconStyle::Yellow)),
        &mut caps,
    );
    run_full_context_effect(
        &state,
        ContextEffect::ApplyGlobalHotkey("Cmd+Shift+B".to_string()),
        &mut caps,
    );
    run_full_context_effect(&state, ContextEffect::QuitApp, &mut caps);

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
