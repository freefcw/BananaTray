use crate::application::AppAction;
use crate::runtime::AppState;
use gpui::{App, Window};
use std::cell::RefCell;
use std::rc::Rc;

use super::settings_window::{
    clear_popup_view, notify_popup_view, notify_settings_view, schedule_open_settings_window,
};

// Window / App 两个 shell 上下文共享的能力实现。render 与 open_settings_window
// 依赖各自上下文（Window 刷新 / display id），差异保留在 impl 内；其余能力
// 与上下文无关，收敛到这里避免两份逐字相同的拷贝。

#[cfg(target_os = "linux")]
fn shared_publish_quota_snapshot(state: &Rc<RefCell<AppState>>, cx: &mut App) {
    super::workers::linux_dbus::emit_registered_dbus_snapshot(state, cx);
}

#[cfg(not(target_os = "linux"))]
fn shared_publish_quota_snapshot(state: &Rc<RefCell<AppState>>, _cx: &mut App) {
    let _ = state;
}

fn shared_apply_tray_icon(cx: &mut App, request: crate::application::TrayIconRequest) {
    crate::tray::apply_tray_icon(cx, request);
}

fn shared_apply_global_hotkey(
    state: &Rc<RefCell<AppState>>,
    hotkey: &str,
    cx: &mut App,
) -> AppAction {
    crate::runtime::global_hotkey::rebind_global_hotkey(state, hotkey, cx)
}

struct WindowShellCaps<'a> {
    window: &'a mut Window,
    cx: &'a mut App,
}

impl crate::runtime::ContextCapabilities for WindowShellCaps<'_> {
    fn render(&mut self) {
        self.window.refresh();
    }

    fn publish_quota_snapshot(&mut self, state: &Rc<RefCell<AppState>>) {
        shared_publish_quota_snapshot(state, self.cx);
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
        shared_apply_tray_icon(self.cx, request);
    }

    fn apply_global_hotkey(&mut self, state: &Rc<RefCell<AppState>>, hotkey: &str) -> AppAction {
        shared_apply_global_hotkey(state, hotkey, self.cx)
    }

    fn quit(&mut self) {
        self.cx.quit();
    }
}

struct AppShellCaps<'a> {
    cx: &'a mut App,
}

impl crate::runtime::ContextCapabilities for AppShellCaps<'_> {
    fn render(&mut self) {
        notify_popup_view(self.cx);
        notify_settings_view(self.cx);
    }

    fn publish_quota_snapshot(&mut self, state: &Rc<RefCell<AppState>>) {
        shared_publish_quota_snapshot(state, self.cx);
    }
}

impl crate::runtime::FullContextCapabilities for AppShellCaps<'_> {
    fn open_settings_window(&mut self, state: &Rc<RefCell<AppState>>) {
        schedule_open_settings_window(state.clone(), None, self.cx);
    }

    fn apply_tray_icon(&mut self, request: crate::application::TrayIconRequest) {
        shared_apply_tray_icon(self.cx, request);
    }

    fn apply_global_hotkey(&mut self, state: &Rc<RefCell<AppState>>, hotkey: &str) -> AppAction {
        shared_apply_global_hotkey(state, hotkey, self.cx)
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
