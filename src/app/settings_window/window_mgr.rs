use super::SettingsView;
use crate::app::AppState;
use crate::models::ProviderKind;
use gpui::*;
use log::{error, info};
use rust_i18n::t;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

// ============================================================================
// macOS: find the display containing the mouse cursor
// ============================================================================

#[cfg(target_os = "macos")]
mod platform_display {
    use gpui::{App, DisplayId};

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGPoint {
        x: f64,
        y: f64,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGSize {
        width: f64,
        height: f64,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGRect {
        origin: CGPoint,
        size: CGSize,
    }

    type CGDirectDisplayID = u32;

    // Opaque pointer for CGEvent
    type CGEventRef = *const std::ffi::c_void;

    extern "C" {
        fn CGEventCreate(source: *const std::ffi::c_void) -> CGEventRef;
        fn CGEventGetLocation(event: CGEventRef) -> CGPoint;
        fn CFRelease(cf: *const std::ffi::c_void);
        fn CGDisplayBounds(display: CGDirectDisplayID) -> CGRect;
    }

    /// Get the global mouse cursor position via CoreGraphics.
    fn mouse_position() -> Option<CGPoint> {
        unsafe {
            let event = CGEventCreate(std::ptr::null());
            if event.is_null() {
                return None;
            }
            let loc = CGEventGetLocation(event);
            CFRelease(event);
            Some(loc)
        }
    }

    /// Find which display the mouse cursor is on by checking CGDisplayBounds
    /// for each display known to GPUI.
    pub fn find_mouse_display(cx: &App) -> Option<DisplayId> {
        let pos = mouse_position()?;
        cx.displays().into_iter().find_map(|d| {
            let id_u32: u32 = d.id().into();
            let rect = unsafe { CGDisplayBounds(id_u32) };
            let contains = pos.x >= rect.origin.x
                && pos.x < rect.origin.x + rect.size.width
                && pos.y >= rect.origin.y
                && pos.y < rect.origin.y + rect.size.height;
            if contains {
                Some(d.id())
            } else {
                None
            }
        })
    }
}

#[cfg(not(target_os = "macos"))]
mod platform_display {
    use gpui::{App, DisplayId};

    pub fn find_mouse_display(_cx: &App) -> Option<DisplayId> {
        None
    }
}

// ============================================================================
// 设置窗口管理
// ============================================================================

thread_local! {
    static SETTINGS_WINDOW: RefCell<Option<WindowHandle<SettingsView>>> = const { RefCell::new(None) };
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

    // Determine target display: prefer provided display_id, then mouse cursor display
    let target_display_id = display_id.or_else(|| platform_display::find_mouse_display(cx));

    // Try to activate an existing settings window first
    let activated_existing = SETTINGS_WINDOW.with(|slot| {
        if let Some(handle) = slot.borrow().as_ref() {
            info!(target: "settings", "existing settings window found, attempting to activate it");

            // Check if window is on a different display than the target;
            // if so, close and reopen on the correct display.
            if let Some(target_id) = target_display_id {
                let on_different_display = handle
                    .update(cx, |_, window, cx| {
                        // window.display() returns the display the window is currently on
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
                    return false;
                }
            }

            let ok = handle
                .update(cx, |_, window, _| {
                    window.show_window();
                    window.activate_window();
                })
                .is_ok();
            if !ok {
                info!(target: "settings", "existing handle is stale, clearing");
            }
            ok
        } else {
            false
        }
    });

    if activated_existing {
        cx.activate(true);
        info!(target: "settings", "activated existing settings window");
        return;
    }

    SETTINGS_WINDOW.with(|slot| {
        *slot.borrow_mut() = None;
    });

    let settings_state = state.clone();
    let display_id = target_display_id;
    let window_size = size(px(640.0), px(700.0));
    // Calculate display-local centered bounds. Bounds::centered() returns global
    // coordinates, but the macOS platform layer adds screen_frame.origin on top,
    // causing double-offset on secondary displays.
    let display_bounds = display_id
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
    let result = cx.open_window(
        WindowOptions {
            window_bounds: Some(window_bounds),
            window_min_size: Some(size(px(560.0), px(500.0))),
            titlebar: Some(TitlebarOptions {
                title: Some(t!("settings.title").to_string().into()),
                ..Default::default()
            }),
            kind: WindowKind::Normal,
            display_id,
            ..Default::default()
        },
        |_window, cx| cx.new(|cx| SettingsView::new(settings_state, cx)),
    );

    if let Ok(handle) = result {
        info!(target: "settings", "opened new settings window");
        cx.activate(true);
        let _ = handle.update(cx, |_, window, _| {
            window.show_window();
            window.activate_window();
            // Force a resize cycle to sync viewport/renderer on secondary displays.
            // The Metal layer drawable size may not match the GPUI viewport after
            // cross-display window creation; triggering a resize corrects this.
            let vp = window.viewport_size();
            window.resize(size(vp.width + px(1.0), vp.height));
            window.resize(vp);
        });
        info!(target: "settings", "requested app/window activation for settings window");
        SETTINGS_WINDOW.with(|slot| {
            *slot.borrow_mut() = Some(handle);
        });
    } else if let Err(err) = result {
        error!(target: "settings", "failed to open settings window: {err:?}");
    }
}

/// 打开设置窗口并选中指定的 Provider
pub fn schedule_open_settings_window_with_provider(
    state: Rc<RefCell<AppState>>,
    provider: ProviderKind,
    display_id: Option<DisplayId>,
    cx: &mut App,
) {
    // 先设置选中的 provider
    state.borrow_mut().settings_ui.selected_provider = provider;
    // 然后打开设置窗口
    schedule_open_settings_window(state, display_id, cx);
}
