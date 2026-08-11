use crate::application::AppAction;
use crate::runtime::AppState;
use gpui::{App, Window};
use std::cell::RefCell;
use std::rc::Rc;

use super::settings_window::{clear_popup_view, notify_popup_view, schedule_open_settings_window};

struct WindowShellCaps<'a> {
    window: &'a mut Window,
    cx: &'a mut App,
}

impl crate::runtime::ContextCapabilities for WindowShellCaps<'_> {
    fn render(&mut self, _state: &Rc<RefCell<AppState>>) {
        self.window.refresh();
    }
}

impl crate::runtime::FullContextCapabilities for WindowShellCaps<'_> {
    fn open_settings_window(&mut self, state: &Rc<RefCell<AppState>>) {
        let display_id = self.window.display(self.cx).map(|display| display.id());
        clear_popup_view(state);
        self.window.remove_window();
        schedule_open_settings_window(state.clone(), display_id, self.cx);
    }

    fn apply_tray_icon(&mut self, request: crate::application::TrayIconRequest) {
        crate::tray::apply_tray_icon(self.cx, request);
    }

    fn apply_global_hotkey(&mut self, state: &Rc<RefCell<AppState>>, hotkey: &str) -> AppAction {
        crate::runtime::global_hotkey::rebind_global_hotkey(state, hotkey, self.cx)
    }

    fn quit(&mut self) {
        self.cx.quit();
    }
}

struct AppShellCaps<'a> {
    cx: &'a mut App,
}

impl crate::runtime::ContextCapabilities for AppShellCaps<'_> {
    fn render(&mut self, state: &Rc<RefCell<AppState>>) {
        notify_popup_view(state, self.cx);
    }
}

impl crate::runtime::FullContextCapabilities for AppShellCaps<'_> {
    fn open_settings_window(&mut self, state: &Rc<RefCell<AppState>>) {
        schedule_open_settings_window(state.clone(), None, self.cx);
    }

    fn apply_tray_icon(&mut self, request: crate::application::TrayIconRequest) {
        crate::tray::apply_tray_icon(self.cx, request);
    }

    fn apply_global_hotkey(&mut self, state: &Rc<RefCell<AppState>>, hotkey: &str) -> AppAction {
        crate::runtime::global_hotkey::rebind_global_hotkey(state, hotkey, self.cx)
    }

    fn quit(&mut self) {
        self.cx.quit();
    }
}

pub(crate) fn dispatch_in_window(
    state: &Rc<RefCell<AppState>>,
    action: AppAction,
    window: &mut Window,
    cx: &mut App,
) {
    crate::runtime::dispatch_with_full_context(state, action, &mut WindowShellCaps { window, cx });
}

pub(crate) fn dispatch_in_app(state: &Rc<RefCell<AppState>>, action: AppAction, cx: &mut App) {
    crate::runtime::dispatch_with_full_context(state, action, &mut AppShellCaps { cx });
}
