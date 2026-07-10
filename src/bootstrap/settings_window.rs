use crate::runtime::AppState;
use gpui::{
    point, px, size, App, Bounds, DisplayId, WindowBounds, WindowHandle, WindowKind, WindowOptions,
};
use log::info;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

type NotifyPopupViewFn = fn(&Rc<RefCell<AppState>>, &mut App);
type BuildSettingsViewFn =
    fn(Rc<RefCell<AppState>>, &mut App) -> gpui::Entity<crate::ui::settings_window::SettingsView>;
type ClearPopupViewFn = fn(&Rc<RefCell<AppState>>);
type SettingsWindowHandle = WindowHandle<crate::ui::settings_window::SettingsView>;

thread_local! {
    static NOTIFY_POPUP_VIEW_FN: RefCell<Option<NotifyPopupViewFn>> = const { RefCell::new(None) };
    static BUILD_SETTINGS_VIEW_FN: RefCell<Option<BuildSettingsViewFn>> = const { RefCell::new(None) };
    static CLEAR_POPUP_VIEW_FN: RefCell<Option<ClearPopupViewFn>> = const { RefCell::new(None) };
    static SETTINGS_WINDOW: RefCell<Option<SettingsWindowHandle>> = const { RefCell::new(None) };
}

pub(crate) fn register_notify_view(f: NotifyPopupViewFn) {
    NOTIFY_POPUP_VIEW_FN.with(|slot| *slot.borrow_mut() = Some(f));
}

pub(crate) fn register_build_settings_view(f: BuildSettingsViewFn) {
    BUILD_SETTINGS_VIEW_FN.with(|slot| *slot.borrow_mut() = Some(f));
}

pub(crate) fn register_clear_popup_view(f: ClearPopupViewFn) {
    CLEAR_POPUP_VIEW_FN.with(|slot| *slot.borrow_mut() = Some(f));
}

pub(super) fn notify_popup_view(state: &Rc<RefCell<AppState>>, cx: &mut App) {
    NOTIFY_POPUP_VIEW_FN.with(|slot| {
        if let Some(f) = *slot.borrow() {
            f(state, cx);
        }
    });
}

pub(crate) fn clear_popup_view(state: &Rc<RefCell<AppState>>) {
    CLEAR_POPUP_VIEW_FN.with(|slot| {
        if let Some(f) = *slot.borrow() {
            f(state);
        }
    });
}

fn build_settings_view(
    state: Rc<RefCell<AppState>>,
    cx: &mut App,
) -> Option<gpui::Entity<crate::ui::settings_window::SettingsView>> {
    BUILD_SETTINGS_VIEW_FN.with(|slot| slot.borrow().map(|f| f(state, cx)))
}

pub(crate) fn schedule_open_settings_window(
    state: Rc<RefCell<AppState>>,
    display_id: Option<DisplayId>,
    cx: &mut App,
) {
    info!(
        target: "settings",
        "scheduled async settings window open (display: {:?})",
        display_id
    );
    let async_cx = cx.to_async();
    let delayed_cx = async_cx.clone();
    async_cx
        .foreground_executor()
        .spawn(async move {
            smol::Timer::after(Duration::from_millis(10)).await;
            let _ = delayed_cx.update(|cx| {
                open_settings_window(state, display_id, cx);
            });
        })
        .detach();
}

fn open_settings_window(state: Rc<RefCell<AppState>>, display_id: Option<DisplayId>, cx: &mut App) {
    info!(target: "settings", "requested settings window");
    let target_display_id = display_id.or_else(|| cx.tray_icon_anchor().map(|a| a.display_id));

    if activate_existing_settings_window(target_display_id, cx) {
        cx.activate(true);
        info!(target: "settings", "activated existing settings window");
        return;
    }

    #[cfg(debug_assertions)]
    SETTINGS_WINDOW.with(|slot| {
        debug_assert!(
            slot.borrow().is_none(),
            "stale settings window slot before opening a new window"
        );
    });

    let Some(build_view) = build_settings_view(state, cx) else {
        log::error!(target: "settings", "settings view factory not registered");
        return;
    };
    let options = settings_window_options(target_display_id, cx);

    match cx.open_window(options, |_window, _cx| build_view) {
        Ok(handle) => install_new_settings_window(handle, cx),
        Err(err) => log::error!(target: "settings", "failed to open settings window: {err:?}"),
    }
}

fn activate_existing_settings_window(target_display_id: Option<DisplayId>, cx: &mut App) -> bool {
    let Some(handle) = SETTINGS_WINDOW.with(|slot| *slot.borrow()) else {
        return false;
    };
    info!(
        target: "settings",
        "existing settings window found, attempting to activate it"
    );

    if settings_window_is_on_different_display(handle, target_display_id, cx) {
        info!(
            target: "settings",
            "window on different display, closing to reopen on target display"
        );
        let _ = handle.update(cx, |_, window, _| window.remove_window());
        clear_settings_window_slot();
        return false;
    }

    let activated = handle
        .update(cx, |_, window, _| {
            window.show_window();
            window.activate_window();
        })
        .is_ok();
    if !activated {
        info!(target: "settings", "existing handle is stale, clearing");
        clear_settings_window_slot();
    }
    activated
}

fn settings_window_is_on_different_display(
    handle: SettingsWindowHandle,
    target_display_id: Option<DisplayId>,
    cx: &mut App,
) -> bool {
    let Some(target_id) = target_display_id else {
        return false;
    };
    handle
        .update(cx, |_, window, cx| {
            window
                .display(cx)
                .map(|display| display.id() != target_id)
                .unwrap_or(true)
        })
        .unwrap_or(false)
}

fn clear_settings_window_slot() {
    SETTINGS_WINDOW.with(|slot| *slot.borrow_mut() = None);
}

fn settings_window_options(target_display_id: Option<DisplayId>, cx: &App) -> WindowOptions {
    let window_size = size(px(600.0), px(640.0));
    let display_bounds = target_display_id
        .and_then(|id| cx.find_display(id))
        .or_else(|| cx.primary_display())
        .map(|display| display.bounds());
    let origin = display_bounds
        .map(|bounds| {
            point(
                bounds.origin.x + (bounds.size.width - window_size.width) / 2.0,
                bounds.origin.y + (bounds.size.height - window_size.height) / 2.0,
            )
        })
        .unwrap_or_else(|| point(px(0.0), px(0.0)));

    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin,
            size: window_size,
        })),
        window_min_size: Some(size(px(460.0), px(520.0))),
        titlebar: None,
        kind: WindowKind::Normal,
        display_id: target_display_id,
        ..Default::default()
    }
}

fn install_new_settings_window(handle: SettingsWindowHandle, cx: &mut App) {
    info!(target: "settings", "opened new settings window");
    cx.activate(true);
    let _ = handle.update(cx, |view, window, cx| {
        window.show_window();
        window.activate_window();
        let viewport = window.viewport_size();
        window.resize(size(viewport.width + px(1.0), viewport.height));
        window.resize(viewport);
        let appearance_sub = cx.observe_window_appearance(window, |_view, _window, cx| {
            cx.notify();
            log::debug!(
                target: "settings",
                "system appearance changed, settings window refreshed"
            );
        });
        view._appearance_sub = Some(appearance_sub);
    });
    info!(target: "settings", "requested app/window activation for settings window");
    SETTINGS_WINDOW.with(|slot| *slot.borrow_mut() = Some(handle));
}
