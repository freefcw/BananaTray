//! Popup window observer registration.

use crate::runtime::AppState;
use crate::tray::activation::{PopupActivationDecision, PopupActivationTracker};
use gpui::{App, WindowHandle};
use log::info;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// 为弹窗窗口注册观察者：失焦自动隐藏 + 系统外观变化同步主题。
pub(super) fn attach_popup_observers(
    state: Rc<RefCell<AppState>>,
    window_slot: Rc<Cell<Option<WindowHandle<crate::ui::AppView>>>>,
    handle: WindowHandle<crate::ui::AppView>,
    cx: &mut App,
) {
    let auto_hide_state = state.clone();
    let activation_tracker = Rc::new(RefCell::new(PopupActivationTracker::default()));
    let _ = handle.update(cx, |view, window, cx| {
        // 监听窗口失焦，自动关闭
        let activation_tracker = activation_tracker.clone();
        let window_slot = window_slot.clone();
        let sub = cx.observe_window_activation(window, move |_view, window, cx| {
            let is_active = window.is_window_active();
            let should_auto_hide = auto_hide_state
                .borrow()
                .session
                .settings
                .system
                .auto_hide_window;
            #[cfg(target_os = "linux")]
            if !is_active {
                let suppression_remaining = auto_hide_state
                    .borrow()
                    .linux_popup_auto_hide_suppression_remaining();
                if let Some(remaining) = suppression_remaining {
                    log::debug!(target: "tray", "delaying deactivation while linux popup drag is active");
                    crate::tray::linux_popup::schedule_auto_hide_recheck(
                        auto_hide_state.clone(),
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
                    auto_hide_state.clone(),
                    window_slot.clone(),
                    activation_tracker.clone(),
                    handle,
                    delay,
                    window,
                    cx,
                );
                return;
            }
            if decision == PopupActivationDecision::Close {
                #[cfg(target_os = "linux")]
                {
                    if window_slot.get() != Some(handle) {
                        return;
                    }
                    info!(target: "tray", "auto-hide hiding inactive tray popup");
                    crate::tray::linux_popup::hide_popup_window(&auto_hide_state, window, cx);
                }
                #[cfg(not(target_os = "linux"))]
                {
                    if !crate::tray::lifecycle::take_window_if_matches(
                        window_slot.as_ref(),
                        handle,
                    ) {
                        return;
                    }
                    info!(target: "tray", "auto-hide closing inactive tray popup");
                    window.remove_window();
                    crate::tray::lifecycle::finalize_popup_close(&auto_hide_state, cx);
                }
            }
        });
        view._activation_sub = Some(sub);

        #[cfg(target_os = "linux")]
        {
            let position_state = view.state.clone();
            let bounds_sub = cx.observe_window_bounds(window, move |_view, window, cx| {
                crate::tray::linux_popup::save_position_if_needed(
                    &position_state,
                    window.bounds(),
                    cx,
                );
            });
            view._bounds_sub = Some(bounds_sub);
        }

        // 监听系统外观变化（深色/浅色模式切换），自动更新主题
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
