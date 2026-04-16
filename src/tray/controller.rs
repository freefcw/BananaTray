//! TrayController — 托盘弹窗窗口生命周期管理
//!
//! 持有全局窗口句柄和 AppState，负责弹窗的打开、关闭、切换等操作。

use crate::application::AppAction;
use crate::models::AppSettings;
use crate::models::NavTab;
use crate::runtime::schedule_open_settings_window;
use crate::runtime::AppState;
use gpui::{
    px, size, App, AppContext, Bounds, DisplayId, Pixels, Size, WindowBounds, WindowHandle,
    WindowKind, WindowOptions, WindowPosition,
};
use log::{debug, error, info};
use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;

/// lib target 不直接构造托盘控制器，但 bin 启动路径会使用它；
/// 收窄到 item 级 suppress，避免继续屏蔽本文件其它未来死代码。
#[allow(dead_code)]
/// 窗口管理器：持有全局窗口句柄，纯数据，不含任何锁操作
pub(crate) struct TrayController {
    window: Rc<Cell<Option<WindowHandle<crate::ui::AppView>>>>,
    pub(crate) state: Rc<RefCell<AppState>>,
}

/// lib target 不直接调用这些方法，但 bin 启动路径与托盘事件会完整覆盖。
#[allow(dead_code)]
impl TrayController {
    pub(crate) fn new(
        refresh_tx: smol::channel::Sender<crate::refresh::RefreshRequest>,
        manager: std::sync::Arc<crate::providers::ProviderManager>,
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
        }
    }

    /// 同步弹窗关闭后的 session 状态。
    fn finalize_popup_close(state: &Rc<RefCell<AppState>>, cx: &mut App) {
        crate::runtime::ui_hooks::clear_popup_view(state);
        // 弹窗关闭后同步动态图标
        crate::runtime::dispatch_in_app(state, AppAction::PopupVisibilityChanged(false), cx);
    }

    /// 仅当 slot 里仍是当前窗口时才清空，避免 auto-hide 误关闭新开的窗口。
    fn take_window_if_matches(
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

    /// Close the tray popup window and clear the view entity reference.
    /// Returns the display ID the popup was on, if available.
    pub(crate) fn close_popup(&mut self, cx: &mut App) -> Option<DisplayId> {
        let window = self.window.take()?;
        let mut display_id = None;
        let _ = window.update(cx, |_, window, cx| {
            display_id = window.display(cx).map(|d| d.id());
            window.remove_window();
        });
        Self::finalize_popup_close(&self.state, cx);
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

    pub(crate) fn toggle_provider(&mut self, cx: &mut App) {
        let (show_overview, provider_tab) = {
            let mut state = self.state.borrow_mut();
            (
                state.session.settings.display.show_overview,
                state.session.default_provider_tab(),
            )
        };

        // Overview 启用时优先展示 Overview tab
        let target_tab = if show_overview {
            Some(NavTab::Overview)
        } else {
            provider_tab
        };

        let Some(target_tab) = target_tab else {
            info!(target: "tray", "no providers enabled, opening settings directly");
            self.show_settings(cx);
            return;
        };
        info!(target: "tray", "toggle provider panel for {:?}", target_tab);

        // Check if window is actually alive, not just if handle exists
        if self.is_window_alive(cx) {
            let active_tab = self.state.borrow().session.nav.active_tab.clone();
            if matches!(active_tab, NavTab::Provider(_) | NavTab::Overview) {
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

        if self.window.get().is_some() {
            info!(target: "tray", "reusing existing tray window");
        } else {
            info!(target: "tray", "opening a fresh tray window");
            self.open(cx);
        }
    }

    fn preferred_window_kind() -> WindowKind {
        if cfg!(target_os = "linux") {
            WindowKind::Floating
        } else {
            WindowKind::PopUp
        }
    }

    /// 计算弹窗的首选位置和目标显示器。
    ///
    /// 使用 GPUI 的 `TrayAnchored` 定位，自动处理多显示器坐标转换。
    fn preferred_window_bounds(
        cx: &App,
        window_size: Size<Pixels>,
    ) -> (Bounds<Pixels>, Option<DisplayId>) {
        if let Some(anchor) = cx
            .tray_icon_anchor()
            .filter(|a| a.bounds.size.width > px(0.0) && a.bounds.size.height > px(0.0))
        {
            debug!(
                target: "tray",
                "tray_icon_anchor: display={:?} origin=({:.1},{:.1}) size=({:.1}x{:.1})",
                anchor.display_id,
                anchor.bounds.origin.x, anchor.bounds.origin.y,
                anchor.bounds.size.width, anchor.bounds.size.height,
            );

            let display_id = anchor.display_id;
            let position = WindowPosition::TrayAnchored(anchor);
            return (
                cx.compute_window_bounds(window_size, &position),
                Some(display_id),
            );
        }

        debug!(target: "tray", "tray anchor unavailable or invalid, using fallback position");
        let position = if cfg!(target_os = "linux") {
            WindowPosition::TopRight { margin: px(16.0) }
        } else {
            WindowPosition::Center
        };

        (cx.compute_window_bounds(window_size, &position), None)
    }

    fn open(&mut self, cx: &mut App) {
        let dynamic_height = self.state.borrow().session.popup_height();
        info!(target: "tray", "opening window with dynamic height: {}px", dynamic_height);
        let window_size = size(px(crate::models::PopupLayout::WIDTH), px(dynamic_height));
        let (bounds, display_id) = Self::preferred_window_bounds(cx, window_size);
        let kind = Self::preferred_window_kind();

        info!(
            target: "tray",
            "popup bounds: origin=({:.1},{:.1}) size=({:.0}x{:.0}), display={:?}",
            bounds.origin.x, bounds.origin.y,
            bounds.size.width, bounds.size.height,
            display_id,
        );

        let state = self.state.clone();

        let result = cx.open_window(
            WindowOptions {
                titlebar: None,
                kind,
                focus: true,
                show: true,
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                display_id,
                ..Default::default()
            },
            |_window, cx| cx.new(|cx| crate::ui::AppView::new(state, cx)),
        );

        if let Ok(handle) = result {
            info!(target: "tray", "tray popup opened successfully");
            // 标记弹窗可见
            crate::runtime::dispatch_in_app(
                &self.state,
                AppAction::PopupVisibilityChanged(true),
                cx,
            );
            self.window.set(Some(handle));
            self.attach_observers(handle, cx);
        } else if let Err(err) = result {
            error!(target: "tray", "failed to open tray popup: {err:?}");
        }
    }

    /// 为弹窗窗口注册观察者：失焦自动隐藏 + 系统外观变化同步主题。
    fn attach_observers(&self, handle: WindowHandle<crate::ui::AppView>, cx: &mut App) {
        let auto_hide_state = self.state.clone();
        let window_slot = self.window.clone();
        let activation_initialized = Rc::new(Cell::new(false));
        let _ = handle.update(cx, |view, window, cx| {
            // 监听窗口失焦，自动关闭
            let activation_initialized = activation_initialized.clone();
            let window_slot = window_slot.clone();
            let sub = cx.observe_window_activation(window, move |_view, window, cx| {
                if !activation_initialized.replace(true) {
                    return;
                }
                let should_auto_hide = auto_hide_state
                    .borrow()
                    .session
                    .settings
                    .system
                    .auto_hide_window;
                if should_auto_hide && !window.is_window_active() {
                    if !Self::take_window_if_matches(window_slot.as_ref(), handle) {
                        return;
                    }
                    info!(target: "tray", "auto-hide closing inactive tray popup");
                    window.remove_window();
                    Self::finalize_popup_close(&auto_hide_state, cx);
                }
            });
            view._activation_sub = Some(sub);

            // 监听系统外观变化（深色/浅色模式切换），自动更新主题
            let appearance_state = view.state.clone();
            let appearance_sub = cx.observe_window_appearance(window, move |_view, window, cx| {
                let user_theme = appearance_state.borrow().session.settings.display.theme;
                let theme =
                    crate::theme::Theme::resolve_for_settings(user_theme, window.appearance());
                cx.set_global(theme);
                log::debug!(target: "app", "system appearance changed, tray theme updated");
            });
            view._appearance_sub = Some(appearance_sub);
        });
    }
}
