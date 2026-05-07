//! Linux-specific tray popup behavior.
//!
//! Linux tray popups use floating windows. Wayland compositors can remap them
//! unpredictably, so this module owns the hidden-but-mapped behavior, drag
//! position persistence, and delayed auto-hide rechecks.

use crate::application::AppAction;
use crate::runtime::AppState;
use crate::tray::activation::{PopupActivationDecision, PopupActivationTracker};
use gpui::{App, Bounds, Pixels, WindowHandle};
use log::{debug, info};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

const AUTO_HIDE_RECHECK_PADDING: Duration = Duration::from_millis(50);

pub(super) fn ensure_popup_visible(handle: WindowHandle<crate::ui::AppView>, cx: &mut App) {
    info!(
        target: "tray",
        "ensuring linux tray popup is shown and activation is requested"
    );
    let _ = handle.update(cx, |_, window, cx| {
        window.set_mouse_passthrough(false);
        window.show_window();
        window.activate_window();
        cx.notify();
    });
}

pub(super) fn hide_popup_window(
    state: &Rc<RefCell<AppState>>,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<crate::ui::AppView>,
) {
    save_position_if_needed(state, window.bounds(), cx);
    let preserve_mapping = {
        let state = state.borrow();
        should_preserve_popup_mapping(&state)
    };
    if preserve_mapping {
        if window.is_window_visible() {
            window.set_mouse_passthrough(true);
        }
    } else if window.is_window_visible() {
        window.hide_window();
    }
    crate::runtime::dispatch_in_window(state, AppAction::PopupVisibilityChanged(false), window, cx);
    cx.notify();
}

pub(super) fn save_position_if_needed(
    state: &Rc<RefCell<AppState>>,
    bounds: Bounds<Pixels>,
    cx: &App,
) {
    if !state.borrow().should_save_linux_popup_position() {
        return;
    }

    let Some(position) = crate::tray::positioning::saved_position_from_bounds(bounds, cx) else {
        return;
    };

    let mut state_ref = state.borrow_mut();
    if state_ref
        .session
        .settings
        .display
        .tray_popup
        .linux_last_position
        == Some(position)
    {
        return;
    }

    state_ref
        .session
        .settings
        .display
        .tray_popup
        .linux_last_position = Some(position);
    state_ref
        .settings_writer
        .schedule(state_ref.session.settings.clone());
    debug!(
        target: "tray",
        "saved linux popup position: ({:.1},{:.1})",
        position.x,
        position.y,
    );
}

pub(super) fn schedule_auto_hide_recheck(
    state: Rc<RefCell<AppState>>,
    window_slot: Rc<Cell<Option<WindowHandle<crate::ui::AppView>>>>,
    activation_tracker: Rc<RefCell<PopupActivationTracker>>,
    handle: WindowHandle<crate::ui::AppView>,
    delay: Duration,
    window: &gpui::Window,
    cx: &mut gpui::Context<crate::ui::AppView>,
) {
    let delay = delay + AUTO_HIDE_RECHECK_PADDING;
    cx.spawn_in(window, async move |_, cx| {
        gpui::Timer::after(delay).await;
        let _ = handle.update(cx, |_, window, cx| {
            if window_slot.get() != Some(handle)
                || !state.borrow().session.popup_visible
                || window.is_window_active()
            {
                return;
            }
            let suppression_remaining =
                state.borrow().linux_popup_auto_hide_suppression_remaining();
            if let Some(remaining) = suppression_remaining {
                debug!(
                    target: "tray",
                    "linux popup auto-hide recheck is still suppressed; scheduling again"
                );
                schedule_auto_hide_recheck(
                    state.clone(),
                    window_slot.clone(),
                    activation_tracker.clone(),
                    handle,
                    remaining,
                    window,
                    cx,
                );
                return;
            }
            let should_auto_hide = state.borrow().session.settings.system.auto_hide_window;
            if activation_tracker
                .borrow_mut()
                .on_activation_event(false, should_auto_hide)
                == PopupActivationDecision::Close
            {
                info!(target: "tray", "auto-hide hiding inactive tray popup after recheck");
                hide_popup_window(&state, window, cx);
            }
        });
    })
    .detach();
}

fn should_preserve_popup_mapping(state: &AppState) -> bool {
    state.should_save_linux_popup_position()
        || state
            .session
            .settings
            .display
            .tray_popup
            .linux_last_position
            .is_some()
}
