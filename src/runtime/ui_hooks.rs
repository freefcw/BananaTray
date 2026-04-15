use crate::runtime::AppState;
use gpui::{App, DisplayId};
use std::cell::RefCell;
use std::rc::Rc;

type NotifyViewFn = fn(&Rc<RefCell<AppState>>, &mut App);
type BuildSettingsFn =
    fn(Rc<RefCell<AppState>>, &mut App) -> gpui::Entity<crate::ui::settings_window::SettingsView>;
type ClearPopupViewFn = fn(&Rc<RefCell<AppState>>);

thread_local! {
    static NOTIFY_VIEW_FN: RefCell<Option<NotifyViewFn>> = const { RefCell::new(None) };
    static BUILD_SETTINGS_FN: RefCell<Option<BuildSettingsFn>> = const { RefCell::new(None) };
    static CLEAR_POPUP_VIEW_FN: RefCell<Option<ClearPopupViewFn>> = const { RefCell::new(None) };
}

#[allow(dead_code)]
pub(crate) fn register_notify_view(f: NotifyViewFn) {
    NOTIFY_VIEW_FN.with(|slot| *slot.borrow_mut() = Some(f));
}

#[allow(dead_code)]
pub(crate) fn register_build_settings_view(f: BuildSettingsFn) {
    BUILD_SETTINGS_FN.with(|slot| *slot.borrow_mut() = Some(f));
}

#[allow(dead_code)]
pub(crate) fn register_clear_popup_view(f: ClearPopupViewFn) {
    CLEAR_POPUP_VIEW_FN.with(|slot| *slot.borrow_mut() = Some(f));
}

pub(crate) fn notify_view(state: &Rc<RefCell<AppState>>, cx: &mut App) {
    NOTIFY_VIEW_FN.with(|slot| {
        if let Some(f) = *slot.borrow() {
            f(state, cx);
        }
    });
}

pub(crate) fn build_settings_view(
    state: Rc<RefCell<AppState>>,
    cx: &mut App,
) -> Option<gpui::Entity<crate::ui::settings_window::SettingsView>> {
    BUILD_SETTINGS_FN.with(|slot| slot.borrow().map(|f| f(state, cx)))
}

pub(crate) fn clear_popup_view(state: &Rc<RefCell<AppState>>) {
    CLEAR_POPUP_VIEW_FN.with(|slot| {
        if let Some(f) = *slot.borrow() {
            f(state);
        }
    });
}

pub(crate) fn _unused_display(_: Option<DisplayId>) {}
