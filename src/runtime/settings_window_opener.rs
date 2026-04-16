use crate::runtime::AppState;
use gpui::{
    point, px, size, App, Bounds, DisplayId, WindowBounds, WindowHandle, WindowKind, WindowOptions,
};
use log::{error, info};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

thread_local! {
    static SETTINGS_WINDOW: RefCell<Option<WindowHandle<crate::ui::settings_window::SettingsView>>> = const { RefCell::new(None) };
}

pub fn schedule_open_settings_window(
    state: Rc<RefCell<AppState>>,
    display_id: Option<DisplayId>,
    cx: &mut App,
) {
    info!(target: "settings", "scheduled async settings window open (display: {:?})", display_id);
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

    let existing_handle = SETTINGS_WINDOW.with(|slot| *slot.borrow());
    let activated_existing = if let Some(handle) = existing_handle {
        info!(target: "settings", "existing settings window found, attempting to activate it");
        let mut should_reopen = false;

        if let Some(target_id) = target_display_id {
            let on_different_display = handle
                .update(cx, |_, window, cx| {
                    window
                        .display(cx)
                        .map(|d| d.id() != target_id)
                        .unwrap_or(true)
                })
                .unwrap_or(false);

            if on_different_display {
                info!(target: "settings", "window on different display, closing to reopen on target display");
                let _ = handle.update(cx, |_, window, _| {
                    window.remove_window();
                });
                SETTINGS_WINDOW.with(|slot| {
                    *slot.borrow_mut() = None;
                });
                should_reopen = true;
            }
        }

        if !should_reopen {
            let ok = handle
                .update(cx, |_, window, _| {
                    window.show_window();
                    window.activate_window();
                })
                .is_ok();
            if !ok {
                info!(target: "settings", "existing handle is stale, clearing");
                SETTINGS_WINDOW.with(|slot| {
                    *slot.borrow_mut() = None;
                });
            }
            ok
        } else {
            false
        }
    } else {
        false
    };

    if activated_existing {
        cx.activate(true);
        info!(target: "settings", "activated existing settings window");
        return;
    }

    // 确保旧 slot 已清空（stale handle 或异常关闭场景）
    SETTINGS_WINDOW.with(|slot| {
        *slot.borrow_mut() = None;
    });

    let settings_state = state.clone();
    let window_size = size(px(600.0), px(640.0));
    // 计算显示器居中位置，避免多屏场景下 Bounds::centered() 全局坐标偏移
    let display_bounds = target_display_id
        .and_then(|id| cx.find_display(id))
        .or_else(|| cx.primary_display())
        .map(|d| d.bounds().size)
        .unwrap_or(window_size);
    let origin = point(
        (display_bounds.width - window_size.width) / 2.0,
        (display_bounds.height - window_size.height) / 2.0,
    );
    let window_bounds = WindowBounds::Windowed(Bounds {
        origin,
        size: window_size,
    });

    let Some(build_view) = crate::runtime::ui_hooks::build_settings_view(settings_state, cx) else {
        error!(target: "settings", "settings view factory not registered");
        return;
    };

    let result = cx.open_window(
        WindowOptions {
            window_bounds: Some(window_bounds),
            window_min_size: Some(size(px(460.0), px(520.0))),
            titlebar: None,
            kind: WindowKind::Normal,
            display_id: target_display_id,
            ..Default::default()
        },
        |_window, _cx| build_view,
    );

    if let Ok(handle) = result {
        info!(target: "settings", "opened new settings window");
        cx.activate(true);
        let _ = handle.update(cx, |view, window, cx| {
            window.show_window();
            window.activate_window();
            let vp = window.viewport_size();
            window.resize(size(vp.width + px(1.0), vp.height));
            window.resize(vp);
            let appearance_sub = cx.observe_window_appearance(window, |_view, _window, cx| {
                cx.notify();
                log::debug!(target: "settings", "system appearance changed, settings window refreshed");
            });
            view._appearance_sub = Some(appearance_sub);
        });
        info!(target: "settings", "requested app/window activation for settings window");
        SETTINGS_WINDOW.with(|slot| {
            *slot.borrow_mut() = Some(handle);
        });
    } else if let Err(err) = result {
        error!(target: "settings", "failed to open settings window: {err:?}");
    }
}
