//! Popup window observer registration.

use crate::runtime::AppState;
use crate::tray::activation::{PopupActivationDecision, PopupActivationTracker};
use gpui::{App, Context, Window, WindowHandle};
use log::info;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

type PopupHandle = WindowHandle<crate::ui::AppView>;

/// 为弹窗窗口注册观察者：失焦自动隐藏 + 系统外观变化同步主题。
pub(super) fn attach_popup_observers(
    state: Rc<RefCell<AppState>>,
    window_slot: Rc<Cell<Option<PopupHandle>>>,
    handle: PopupHandle,
    cx: &mut App,
) {
    let activation_tracker = Rc::new(RefCell::new(PopupActivationTracker::default()));
    attach_activation_observer(state, window_slot, activation_tracker, handle, cx);
    attach_bounds_observer(handle, cx);
    attach_appearance_observer(handle, cx);
}

fn attach_activation_observer(
    state: Rc<RefCell<AppState>>,
    window_slot: Rc<Cell<Option<PopupHandle>>>,
    activation_tracker: Rc<RefCell<PopupActivationTracker>>,
    handle: PopupHandle,
    cx: &mut App,
) {
    let _ = handle.update(cx, |view, window, cx| {
        let sub = cx.observe_window_activation(window, move |_view, window, cx| {
            handle_activation_event(
                &state,
                &window_slot,
                &activation_tracker,
                handle,
                window,
                cx,
            );
        });
        view._activation_sub = Some(sub);
    });
}

fn handle_activation_event(
    state: &Rc<RefCell<AppState>>,
    window_slot: &Rc<Cell<Option<PopupHandle>>>,
    activation_tracker: &Rc<RefCell<PopupActivationTracker>>,
    handle: PopupHandle,
    window: &mut Window,
    cx: &mut Context<crate::ui::AppView>,
) {
    let is_active = window.is_window_active();
    let should_auto_hide = state.borrow().session.settings.system.auto_hide_window;

    #[cfg(target_os = "linux")]
    if !is_active {
        if let Some(remaining) = state.borrow().linux_popup_auto_hide_suppression_remaining() {
            log::debug!(target: "tray", "delaying deactivation while linux popup drag is active");
            crate::tray::linux_popup::schedule_auto_hide_recheck(
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
    }

    let decision = activation_tracker
        .borrow_mut()
        .on_activation_event(is_active, should_auto_hide);
    #[cfg(target_os = "linux")]
    if let PopupActivationDecision::RecheckAfter(delay) = decision {
        crate::tray::linux_popup::schedule_auto_hide_recheck(
            state.clone(),
            window_slot.clone(),
            activation_tracker.clone(),
            handle,
            delay,
            window,
            cx,
        );
        return;
    }
    if decision != PopupActivationDecision::Close {
        return;
    }

    #[cfg(target_os = "linux")]
    {
        if window_slot.get() != Some(handle) {
            return;
        }
        info!(target: "tray", "auto-hide hiding inactive tray popup");
        crate::tray::linux_popup::hide_popup_window(state, window, cx);
    }
    #[cfg(not(target_os = "linux"))]
    {
        if !crate::tray::lifecycle::take_window_if_matches(window_slot.as_ref(), handle) {
            return;
        }
        info!(target: "tray", "auto-hide closing inactive tray popup");
        window.remove_window();
        crate::tray::lifecycle::finalize_popup_close(state, cx);
    }
}

#[cfg(target_os = "linux")]
fn attach_bounds_observer(handle: PopupHandle, cx: &mut App) {
    let _ = handle.update(cx, |view, window, cx| {
        let position_state = view.state.clone();
        let bounds_sub = cx.observe_window_bounds(window, move |_view, window, cx| {
            crate::tray::linux_popup::save_position_if_needed(&position_state, window.bounds(), cx);
        });
        view._bounds_sub = Some(bounds_sub);
    });
}

#[cfg(not(target_os = "linux"))]
fn attach_bounds_observer(_handle: PopupHandle, _cx: &mut App) {}

fn attach_appearance_observer(handle: PopupHandle, cx: &mut App) {
    let _ = handle.update(cx, |view, window, cx| {
        let appearance_state = view.state.clone();
        let appearance_sub = cx.observe_window_appearance(window, move |_view, window, cx| {
            let user_theme = appearance_state.borrow().session.settings.display.theme;
            let theme = crate::theme::Theme::resolve_for_settings(user_theme, window.appearance());
            cx.set_global(theme);
            log::debug!(target: "app", "system appearance changed, tray theme updated");
        });
        view._appearance_sub = Some(appearance_sub);
    });
}
