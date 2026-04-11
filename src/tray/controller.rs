#![allow(dead_code)]
//! TrayController — 托盘弹窗窗口生命周期管理
//!
//! 持有全局窗口句柄和 AppState，负责弹窗的打开、关闭、切换等操作。

use crate::application::AppAction;
use crate::models::AppSettings;
use crate::models::NavTab;
use crate::ui::{schedule_open_settings_window, AppState};
use gpui::*;
use log::{debug, error, info};
use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;

/// 窗口管理器：持有全局窗口句柄，纯数据，不含任何锁操作
pub(crate) struct TrayController {
    window: Option<WindowHandle<crate::ui::AppView>>,
    pub(crate) state: Rc<RefCell<AppState>>,
}

impl TrayController {
    pub(crate) fn new(
        refresh_tx: smol::channel::Sender<crate::refresh::RefreshRequest>,
        manager: &crate::providers::ProviderManager,
        settings: AppSettings,
        log_path: Option<std::path::PathBuf>,
    ) -> Self {
        info!(target: "tray", "initializing tray controller");
        let state = Rc::new(RefCell::new(AppState::new(
            refresh_tx, manager, settings, log_path,
        )));
        Self {
            window: None,
            state,
        }
    }

    /// Close the tray popup window and clear the view entity reference.
    /// Returns the display ID the popup was on, if available.
    pub(crate) fn close_popup(&mut self, cx: &mut App) -> Option<DisplayId> {
        let window = self.window.take()?;
        self.state.borrow_mut().view_entity = None;
        let mut display_id = None;
        let _ = window.update(cx, |_, window, cx| {
            display_id = window.display(cx).map(|d| d.id());
            window.remove_window();
        });
        // 弹窗关闭后同步动态图标
        crate::runtime::dispatch_in_app(&self.state, AppAction::PopupVisibilityChanged(false), cx);
        display_id
    }

    /// Check if the window handle is actually valid (window still exists).
    fn is_window_alive(&self, cx: &mut App) -> bool {
        if let Some(handle) = self.window.as_ref() {
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
            self.window = None;
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

        if self.window.is_some() {
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
    /// macOS 上使用 CoreGraphics 直接定位，避免 GPUI 内部
    /// mainScreen/primaryScreen 高度不一致导致的多屏偏移问题。
    fn preferred_window_bounds(
        cx: &App,
        window_size: Size<Pixels>,
    ) -> (Bounds<Pixels>, Option<DisplayId>) {
        let tray_bounds = cx
            .tray_icon_bounds()
            .filter(|b| b.size.width > px(0.0) && b.size.height > px(0.0));

        if let Some(tray_bounds) = tray_bounds {
            debug!(
                target: "tray",
                "tray_icon_bounds: origin=({:.1},{:.1}) size=({:.1}x{:.1})",
                tray_bounds.origin.x, tray_bounds.origin.y,
                tray_bounds.size.width, tray_bounds.size.height,
            );

            #[cfg(target_os = "macos")]
            {
                return super::display::compute_tray_popup_bounds(cx, window_size, tray_bounds);
            }

            #[cfg(not(target_os = "macos"))]
            {
                let position = WindowPosition::TrayCenter(tray_bounds);
                return (cx.compute_window_bounds(window_size, &position), None);
            }
        }

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
            // 监听窗口失焦，自动关闭
            let auto_hide_state = self.state.clone();
            let activation_initialized = Rc::new(Cell::new(false));
            let _ = handle.update(cx, |view, window, cx| {
                // 监听窗口失焦，自动关闭
                let activation_initialized = activation_initialized.clone();
                let sub = cx.observe_window_activation(window, move |_view, window, _cx| {
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
                        info!(target: "tray", "auto-hide closing inactive tray popup");
                        auto_hide_state.borrow_mut().view_entity = None;
                        crate::runtime::dispatch_in_app(
                            &auto_hide_state,
                            AppAction::PopupVisibilityChanged(false),
                            _cx,
                        );
                        window.remove_window();
                    }
                });
                view._activation_sub = Some(sub);

                // 监听系统外观变化（深色/浅色模式切换），自动更新主题
                let appearance_state = view.state.clone();
                let appearance_sub =
                    cx.observe_window_appearance(window, move |_view, window, cx| {
                        let user_theme = appearance_state.borrow().session.settings.display.theme;
                        let theme = crate::theme::Theme::resolve_for_settings(
                            user_theme,
                            window.appearance(),
                        );
                        cx.set_global(theme);
                        log::debug!(target: "app", "system appearance changed, tray theme updated");
                    });
                view._appearance_sub = Some(appearance_sub);
            });
            self.window = Some(handle);
        } else if let Err(err) = result {
            error!(target: "tray", "failed to open tray popup: {err:?}");
        }
    }
}
