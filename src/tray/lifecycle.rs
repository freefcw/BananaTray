//! Shared popup lifecycle helpers.

use crate::application::AppAction;
use crate::runtime::AppState;
use gpui::App;
#[cfg(not(target_os = "linux"))]
use gpui::WindowHandle;
#[cfg(not(target_os = "linux"))]
use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;

/// 同步弹窗关闭后的 session 状态。
pub(super) fn finalize_popup_close(state: &Rc<RefCell<AppState>>, cx: &mut App) {
    crate::runtime::ui_hooks::clear_popup_view(state);
    // 弹窗关闭后同步动态图标
    crate::runtime::dispatch_in_app(state, AppAction::PopupVisibilityChanged(false), cx);
}

/// 仅当 slot 里仍是当前窗口时才清空，避免 auto-hide 误关闭新开的窗口。
#[cfg(not(target_os = "linux"))]
pub(super) fn take_window_if_matches(
    window_slot: &Cell<Option<WindowHandle<crate::ui::AppView>>>,
    expected: WindowHandle<crate::ui::AppView>,
) -> bool {
    match window_slot.get() {
        Some(current) if current == expected => {
            window_slot.set(None);
            true
        }
        _ => false,
    }
}
