//! TrayController — 托盘弹窗窗口生命周期管理
//!
//! 持有全局窗口句柄和 AppState，负责弹窗的打开、关闭、切换等操作。

use crate::application::AppAction;
use crate::models::{AppSettings, NavTab};
use crate::runtime::schedule_open_settings_window;
use crate::runtime::AppState;
#[cfg(target_os = "linux")]
use crate::tray::activation::GRACE_PERIOD;
use crate::tray::command::ProviderToggleTarget;
use gpui::{
    px, size, App, AppContext, DisplayId, Pixels, Point, WindowBounds, WindowHandle, WindowKind,
    WindowOptions,
};
use log::{error, info};
use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;

/// 窗口管理器：持有全局窗口句柄，纯数据，不含任何锁操作
pub(crate) struct TrayController {
    window: Rc<Cell<Option<WindowHandle<crate::ui::AppView>>>>,
    state: Rc<RefCell<AppState>>,
    /// 最近一次 tray 点击的屏幕坐标（Linux 用于构造 TrayAnchor）
    last_click_position: Cell<Option<Point<Pixels>>>,
}

/// lib target 不直接调用这些方法，但 bin 启动路径与托盘事件会完整覆盖。
#[allow(dead_code)]
impl TrayController {
    pub(crate) fn new(
        refresh_tx: smol::channel::Sender<crate::refresh::RefreshRequest>,
        manager: crate::providers::ProviderManagerHandle,
        settings: AppSettings,
        log_path: Option<std::path::PathBuf>,
    ) -> Self {
        info!(target: "tray", "initializing tray controller");
        let state = Rc::new(RefCell::new(AppState::new(
            refresh_tx, manager, settings, log_path,
        )));
        Self {
            window: Rc::new(Cell::new(None)),
            state,
            last_click_position: Cell::new(None),
        }
    }

    pub(crate) fn state(&self) -> Rc<RefCell<AppState>> {
        self.state.clone()
    }

    /// Hide or close the tray popup window.
    /// Returns the display ID the popup was on, if available.
    pub(crate) fn close_popup(&mut self, cx: &mut App) -> Option<DisplayId> {
        #[cfg(target_os = "linux")]
        {
            self.hide_popup(cx)
        }

        #[cfg(not(target_os = "linux"))]
        {
            let window = self.window.take()?;
            let mut display_id = None;
            let _ = window.update(cx, |_, window, cx| {
                display_id = window.display(cx).map(|d| d.id());
                window.remove_window();
            });
            crate::tray::lifecycle::finalize_popup_close(&self.state, cx);
            display_id
        }
    }

    #[cfg(target_os = "linux")]
    fn hide_popup(&mut self, cx: &mut App) -> Option<DisplayId> {
        let window = self.window.get()?;
        let mut display_id = None;
        let state = self.state.clone();
        let result = window.update(cx, |_, window, cx| {
            display_id = window.display(cx).map(|d| d.id());
            crate::tray::linux_popup::hide_popup_window(&state, window, cx);
        });

        if result.is_err() {
            self.window.set(None);
            crate::tray::lifecycle::finalize_popup_close(&self.state, cx);
        }

        display_id
    }

    /// Check if the window handle is actually valid (window still exists).
    fn is_window_alive(&self, cx: &mut App) -> bool {
        if let Some(handle) = self.window.get() {
            // Try to update the window - if this fails, the handle is stale
            handle.update(cx, |_, _, _| {}).is_ok()
        } else {
            false
        }
    }

    fn is_window_visible(&self, cx: &mut App) -> bool {
        self.window
            .get()
            .and_then(|handle| {
                handle
                    .update(cx, |_, window, _| window.is_window_visible())
                    .ok()
            })
            .unwrap_or(false)
    }

    fn is_popup_visible(&self, cx: &mut App) -> bool {
        self.is_window_visible(cx) && self.state.borrow().session.popup_visible
    }

    /// 记录最近一次 tray 点击的屏幕坐标（由 on_tray_icon_click_event 提供）
    pub(crate) fn set_click_position(&self, position: Option<Point<Pixels>>) {
        self.last_click_position.set(position);
    }

    pub(crate) fn toggle_provider(&mut self, cx: &mut App) {
        let target = {
            let mut state = self.state.borrow_mut();
            crate::tray::command::provider_toggle_target(&mut state.session)
        };

        let target_tab = match target {
            ProviderToggleTarget::Show(tab) => tab,
            ProviderToggleTarget::OpenSettings => {
                info!(target: "tray", "no providers enabled, opening settings directly");
                self.show_settings(cx);
                return;
            }
        };
        info!(target: "tray", "toggle provider panel for {:?}", target_tab);

        // Check if window is actually alive, not just if handle exists
        if self.is_window_alive(cx) {
            let popup_visible = self.is_popup_visible(cx);
            let active_tab = self.state.borrow().session.nav.active_tab.clone();
            if popup_visible && matches!(active_tab, NavTab::Provider(_) | NavTab::Overview) {
                info!(target: "tray", "provider panel already open, closing existing panel");
                self.close_popup(cx);
            } else {
                info!(target: "tray", "reusing existing window handle for provider panel");
                self.show(target_tab, cx);
            }
        } else {
            // Handle is stale, clear it
            info!(target: "tray", "window handle is stale, clearing and opening fresh panel");
            self.window.set(None);
            self.show(target_tab, cx);
        }
    }

    pub(crate) fn show_settings(&mut self, cx: &mut App) {
        info!(target: "tray", "requested settings window from tray controller");
        let display_id = self.close_popup(cx);
        schedule_open_settings_window(self.state.clone(), display_id, cx);
    }

    fn show(&mut self, tab: NavTab, cx: &mut App) {
        info!(target: "tray", "show window for tab {:?}", tab);
        crate::runtime::dispatch_in_app(&self.state, AppAction::SelectNavTab(tab), cx);

        if let Some(handle) = self.window.get() {
            info!(target: "tray", "reusing existing tray window");
            if handle.update(cx, |_, _, _| {}).is_ok() {
                self.show_existing_popup(handle, cx);
            } else {
                info!(target: "tray", "window handle is stale, opening a fresh tray window");
                self.window.set(None);
                self.open(cx);
            }
        } else {
            info!(target: "tray", "opening a fresh tray window");
            self.open(cx);
        }
    }

    fn show_existing_popup(&self, handle: WindowHandle<crate::ui::AppView>, cx: &mut App) {
        #[cfg(target_os = "linux")]
        self.state
            .borrow_mut()
            .suppress_linux_popup_auto_hide_for(GRACE_PERIOD);
        crate::runtime::dispatch_in_app(&self.state, AppAction::PopupVisibilityChanged(true), cx);
        let _ = handle.update(cx, |_, window, cx| {
            #[cfg(target_os = "linux")]
            window.set_mouse_passthrough(false);
            if !window.is_window_visible() {
                window.show_window();
            }
            window.activate_window();
            cx.notify();
        });
        Self::ensure_popup_visible(handle, cx);
    }

    fn preferred_window_kind() -> WindowKind {
        if cfg!(target_os = "linux") {
            WindowKind::Floating
        } else {
            WindowKind::PopUp
        }
    }

    #[cfg(target_os = "linux")]
    fn ensure_popup_visible(handle: WindowHandle<crate::ui::AppView>, cx: &mut App) {
        crate::tray::linux_popup::ensure_popup_visible(handle, cx);
    }

    #[cfg(not(target_os = "linux"))]
    fn ensure_popup_visible(_handle: WindowHandle<crate::ui::AppView>, _cx: &mut App) {}

    /// 计算弹窗的首选位置和目标显示器。
    ///
    /// 优先级：
    /// 1. Linux: 用户拖动后的上次位置
    /// 2. macOS: `tray_icon_anchor()`（系统原生锚点）
    /// 3. Linux: `tray_anchor_for_position()`（从 SNI 点击坐标构造锚点）
    /// 4. fallback: TopRight（Linux）/ Center（macOS）
    fn preferred_window_bounds(
        &self,
        cx: &App,
        window_size: gpui::Size<Pixels>,
    ) -> (gpui::Bounds<Pixels>, Option<DisplayId>) {
        crate::tray::positioning::preferred_window_bounds(
            cx,
            crate::tray::positioning::PopupPositionInputs {
                window_size,
                last_click_position: self.last_click_position.get(),
                saved_position: self
                    .state
                    .borrow()
                    .session
                    .settings
                    .display
                    .tray_popup
                    .linux_last_position,
            },
        )
    }

    fn open(&mut self, cx: &mut App) {
        let dynamic_height = self.state.borrow().session.popup_height();
        info!(target: "tray", "opening window with dynamic height: {}px", dynamic_height);
        let window_size = size(px(crate::models::PopupLayout::WIDTH), px(dynamic_height));
        let (bounds, display_id) = self.preferred_window_bounds(cx, window_size);
        let kind = Self::preferred_window_kind();

        info!(
            target: "tray",
            "popup bounds: origin=({:.1},{:.1}) size=({:.0}x{:.0}), display={:?}",
            bounds.origin.x, bounds.origin.y,
            bounds.size.width, bounds.size.height,
            display_id,
        );

        let state = self.state.clone();
        let options = WindowOptions {
            titlebar: None,
            kind,
            focus: true,
            show: true,
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            display_id,
            #[cfg(target_os = "linux")]
            window_background: gpui::WindowBackgroundAppearance::Transparent,
            ..Default::default()
        };

        let result = cx.open_window(options, |_window, cx| {
            cx.new(|cx| crate::ui::AppView::new(state, cx))
        });

        if let Ok(handle) = result {
            info!(target: "tray", "tray popup opened successfully");
            // 标记弹窗可见
            crate::runtime::dispatch_in_app(
                &self.state,
                AppAction::PopupVisibilityChanged(true),
                cx,
            );
            self.window.set(Some(handle));
            crate::tray::observers::attach_popup_observers(
                self.state.clone(),
                self.window.clone(),
                handle,
                cx,
            );
            Self::ensure_popup_visible(handle, cx);
        } else if let Err(err) = result {
            error!(target: "tray", "failed to open tray popup: {err:?}");
        }
    }
}
