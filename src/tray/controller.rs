//! TrayController — 托盘弹窗窗口生命周期管理
//!
//! 持有全局窗口句柄和 AppState，负责弹窗的打开、关闭、切换等操作。

use crate::application::AppAction;
#[cfg(target_os = "linux")]
use crate::models::SavedWindowPosition;
use crate::models::{AppSettings, NavTab};
use crate::runtime::schedule_open_settings_window;
use crate::runtime::AppState;
use gpui::{
    px, size, App, AppContext, Bounds, DisplayId, Pixels, Point, Size, WindowBounds, WindowHandle,
    WindowKind, WindowOptions, WindowPosition,
};
use log::{debug, error, info};
use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

/// lib target 不直接构造托盘控制器，但 bin 启动路径会使用它；
/// 收窄到 item 级 suppress，避免继续屏蔽本文件其它未来死代码。
#[allow(dead_code)]
/// 窗口管理器：持有全局窗口句柄，纯数据，不含任何锁操作
pub(crate) struct TrayController {
    window: Rc<Cell<Option<WindowHandle<crate::ui::AppView>>>>,
    pub(crate) state: Rc<RefCell<AppState>>,
    /// 最近一次 tray 点击的屏幕坐标（Linux 用于构造 TrayAnchor）
    last_click_position: Cell<Option<Point<Pixels>>>,
}

/// 跟踪 popup 的激活状态，避免 Linux/Wayland 上焦点抖动导致弹窗被误关。
///
/// Wayland compositor 在浮动窗口打开时会产生快速的 focus→unfocus 抖动，
/// tracker 通过 grace period 忽略窗口创建后短时间内的 deactivation 事件。
#[derive(Debug, Clone)]
struct PopupActivationTracker {
    /// 窗口创建时间，用于计算 grace period
    created_at: std::time::Instant,
    /// 收到的 activation 事件计数（用于区分初始闪烁和真实交互）
    event_count: u32,
    /// 窗口是否曾经处于激活状态
    has_been_active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PopupActivationDecision {
    KeepOpen,
    Close,
    RecheckAfter(Duration),
}

/// 窗口创建后的保护期：在此期间内忽略 deactivation 事件。
/// Wayland 上焦点抖动通常在 200ms 内完成，600ms 留足余量。
const GRACE_PERIOD: Duration = Duration::from_millis(600);
#[cfg(target_os = "linux")]
const LINUX_AUTO_HIDE_RECHECK_PADDING: Duration = Duration::from_millis(50);

impl Default for PopupActivationTracker {
    fn default() -> Self {
        Self {
            created_at: std::time::Instant::now(),
            event_count: 0,
            has_been_active: false,
        }
    }
}

impl PopupActivationTracker {
    fn on_activation_event(
        &mut self,
        is_active: bool,
        should_auto_hide: bool,
    ) -> PopupActivationDecision {
        self.event_count += 1;

        if is_active {
            self.has_been_active = true;
            return PopupActivationDecision::KeepOpen;
        }

        // 保护期内忽略 deactivation——Wayland compositor 在窗口创建阶段
        // 可能发出快速 focus→unfocus 抖动，不应解释为用户离开窗口。
        if let Some(remaining) = GRACE_PERIOD.checked_sub(self.created_at.elapsed()) {
            debug!(
                target: "tray",
                "ignoring deactivation during grace period (event #{}, elapsed={:?})",
                self.event_count,
                self.created_at.elapsed(),
            );
            return PopupActivationDecision::RecheckAfter(remaining);
        }

        if should_auto_hide && self.has_been_active {
            PopupActivationDecision::Close
        } else {
            PopupActivationDecision::KeepOpen
        }
    }
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
            Self::finalize_popup_close(&self.state, cx);
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
            Self::hide_linux_popup_window(&state, window, cx);
        });

        if result.is_err() {
            self.window.set(None);
            Self::finalize_popup_close(&self.state, cx);
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
        info!(
            target: "tray",
            "ensuring linux tray popup is shown and activation is requested"
        );
        let _ = handle.update(cx, |_, window, cx| {
            window.set_mouse_passthrough(false);
            window.show_window();
            window.activate_window();
            cx.notify();
        });
    }

    #[cfg(not(target_os = "linux"))]
    fn ensure_popup_visible(_handle: WindowHandle<crate::ui::AppView>, _cx: &mut App) {}

    #[cfg(target_os = "linux")]
    fn saved_popup_bounds(
        &self,
        cx: &App,
        window_size: Size<Pixels>,
    ) -> Option<(Bounds<Pixels>, Option<DisplayId>)> {
        let saved = self
            .state
            .borrow()
            .session
            .settings
            .display
            .tray_popup
            .linux_last_position?;
        if !saved.x.is_finite() || !saved.y.is_finite() {
            return None;
        }

        let origin = gpui::point(px(saved.x), px(saved.y));
        let bounds = Bounds::new(origin, window_size);
        let center = gpui::point(
            origin.x + window_size.width * 0.5,
            origin.y + window_size.height * 0.5,
        );
        let display = cx.displays().into_iter().find(|display| {
            let display_bounds = display.bounds();
            display_bounds.contains(&origin) || display_bounds.contains(&center)
        })?;

        debug!(
            target: "tray",
            "using saved linux popup position on display {:?}: origin=({:.1},{:.1})",
            display.id(),
            bounds.origin.x,
            bounds.origin.y,
        );
        Some((bounds, Some(display.id())))
    }

    #[cfg(target_os = "linux")]
    fn saved_position_from_bounds(bounds: Bounds<Pixels>, cx: &App) -> Option<SavedWindowPosition> {
        let x = f32::from(bounds.origin.x);
        let y = f32::from(bounds.origin.y);
        if !x.is_finite() || !y.is_finite() {
            return None;
        }

        let center = gpui::point(
            bounds.origin.x + bounds.size.width * 0.5,
            bounds.origin.y + bounds.size.height * 0.5,
        );
        let on_display = cx.displays().into_iter().any(|display| {
            let display_bounds = display.bounds();
            display_bounds.contains(&bounds.origin) || display_bounds.contains(&center)
        });
        if !on_display {
            return None;
        }

        Some(SavedWindowPosition { x, y })
    }

    #[cfg(target_os = "linux")]
    fn save_linux_popup_position_if_needed(
        state: &Rc<RefCell<AppState>>,
        bounds: Bounds<Pixels>,
        cx: &App,
    ) {
        if !state.borrow().should_save_linux_popup_position() {
            return;
        }

        let Some(position) = Self::saved_position_from_bounds(bounds, cx) else {
            return;
        };

        let mut state_ref = state.borrow_mut();
        if state_ref
            .session
            .settings
            .display
            .tray_popup
            .linux_last_position
            == Some(position)
        {
            return;
        }

        state_ref
            .session
            .settings
            .display
            .tray_popup
            .linux_last_position = Some(position);
        state_ref
            .settings_writer
            .schedule(state_ref.session.settings.clone());
        debug!(
            target: "tray",
            "saved linux popup position: ({:.1},{:.1})",
            position.x,
            position.y,
        );
    }

    #[cfg(target_os = "linux")]
    fn should_preserve_linux_popup_mapping(state: &AppState) -> bool {
        state.should_save_linux_popup_position()
            || state
                .session
                .settings
                .display
                .tray_popup
                .linux_last_position
                .is_some()
    }

    #[cfg(target_os = "linux")]
    fn hide_linux_popup_window(
        state: &Rc<RefCell<AppState>>,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<crate::ui::AppView>,
    ) {
        Self::save_linux_popup_position_if_needed(state, window.bounds(), cx);
        let preserve_mapping = {
            let state = state.borrow();
            Self::should_preserve_linux_popup_mapping(&state)
        };
        if preserve_mapping {
            if window.is_window_visible() {
                window.set_mouse_passthrough(true);
            }
        } else if window.is_window_visible() {
            window.hide_window();
        }
        crate::runtime::dispatch_in_window(
            state,
            AppAction::PopupVisibilityChanged(false),
            window,
            cx,
        );
        cx.notify();
    }

    #[cfg(target_os = "linux")]
    fn schedule_linux_auto_hide_recheck(
        state: Rc<RefCell<AppState>>,
        window_slot: Rc<Cell<Option<WindowHandle<crate::ui::AppView>>>>,
        activation_tracker: Rc<RefCell<PopupActivationTracker>>,
        handle: WindowHandle<crate::ui::AppView>,
        delay: Duration,
        window: &gpui::Window,
        cx: &mut gpui::Context<crate::ui::AppView>,
    ) {
        let delay = delay + LINUX_AUTO_HIDE_RECHECK_PADDING;
        cx.spawn_in(window, async move |_, cx| {
            gpui::Timer::after(delay).await;
            let _ = handle.update(cx, |_, window, cx| {
                if window_slot.get() != Some(handle)
                    || !state.borrow().session.popup_visible
                    || window.is_window_active()
                {
                    return;
                }
                let suppression_remaining =
                    state.borrow().linux_popup_auto_hide_suppression_remaining();
                if let Some(remaining) = suppression_remaining {
                    debug!(
                        target: "tray",
                        "linux popup auto-hide recheck is still suppressed; scheduling again"
                    );
                    Self::schedule_linux_auto_hide_recheck(
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
                let should_auto_hide = state.borrow().session.settings.system.auto_hide_window;
                if activation_tracker
                    .borrow_mut()
                    .on_activation_event(false, should_auto_hide)
                    == PopupActivationDecision::Close
                {
                    info!(target: "tray", "auto-hide hiding inactive tray popup after recheck");
                    Self::hide_linux_popup_window(&state, window, cx);
                }
            });
        })
        .detach();
    }

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
        window_size: Size<Pixels>,
    ) -> (Bounds<Pixels>, Option<DisplayId>) {
        #[cfg(target_os = "linux")]
        if let Some(saved) = self.saved_popup_bounds(cx, window_size) {
            return saved;
        }

        // 优先使用系统原生锚点（macOS 始终可用）
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

        // Linux: 用 SNI 点击坐标构造近似锚点
        if let Some(anchor) = self
            .last_click_position
            .get()
            .and_then(|pos| cx.tray_anchor_for_position(pos))
        {
            debug!(
                target: "tray",
                "tray_anchor_for_position: display={:?} bounds=({:.1},{:.1} {:.1}x{:.1})",
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

        debug!(target: "tray", "tray anchor unavailable and no click position, using fallback");

        if cfg!(target_os = "linux") {
            // Wayland 的 primary_display() 返回 None，compute_window_bounds 的
            // TopRight 路径会退化到 (0,0)。直接取第一个显示器手动计算。
            if let Some(display) = cx.displays().into_iter().next() {
                let db = display.bounds();
                let margin = px(16.0);
                let origin = gpui::point(
                    db.origin.x + db.size.width - window_size.width - margin,
                    db.origin.y + margin,
                );
                let bounds = Bounds::new(origin, window_size);
                debug!(
                    target: "tray",
                    "fallback TopRight on display {:?}: origin=({:.1},{:.1})",
                    display.id(), bounds.origin.x, bounds.origin.y,
                );
                return (bounds, Some(display.id()));
            }
            // 连 displays() 都为空（不太可能），最终 fallback
            (
                Bounds::new(gpui::point(px(0.0), px(0.0)), window_size),
                None,
            )
        } else {
            let position = WindowPosition::Center;
            (cx.compute_window_bounds(window_size, &position), None)
        }
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
        let mut options = WindowOptions {
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
            self.attach_observers(handle, cx);
            Self::ensure_popup_visible(handle, cx);
        } else if let Err(err) = result {
            error!(target: "tray", "failed to open tray popup: {err:?}");
        }
    }

    /// 为弹窗窗口注册观察者：失焦自动隐藏 + 系统外观变化同步主题。
    fn attach_observers(&self, handle: WindowHandle<crate::ui::AppView>, cx: &mut App) {
        let auto_hide_state = self.state.clone();
        let window_slot = self.window.clone();
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
                        debug!(target: "tray", "delaying deactivation while linux popup drag is active");
                        Self::schedule_linux_auto_hide_recheck(
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
                    Self::schedule_linux_auto_hide_recheck(
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
                        Self::hide_linux_popup_window(&auto_hide_state, window, cx);
                    }
                    #[cfg(not(target_os = "linux"))]
                    {
                        if !Self::take_window_if_matches(window_slot.as_ref(), handle) {
                            return;
                        }
                        info!(target: "tray", "auto-hide closing inactive tray popup");
                        window.remove_window();
                        Self::finalize_popup_close(&auto_hide_state, cx);
                    }
                }
            });
            view._activation_sub = Some(sub);

            #[cfg(target_os = "linux")]
            {
                let position_state = view.state.clone();
                let bounds_sub = cx.observe_window_bounds(window, move |_view, window, cx| {
                    Self::save_linux_popup_position_if_needed(
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
                let theme =
                    crate::theme::Theme::resolve_for_settings(user_theme, window.appearance());
                cx.set_global(theme);
                log::debug!(target: "app", "system appearance changed, tray theme updated");
            });
            view._appearance_sub = Some(appearance_sub);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{PopupActivationDecision, PopupActivationTracker, GRACE_PERIOD};
    use std::time::Instant;

    /// 创建一个已过保护期的 tracker，用于测试 auto-hide 逻辑
    fn tracker_past_grace() -> PopupActivationTracker {
        PopupActivationTracker {
            created_at: Instant::now() - GRACE_PERIOD - std::time::Duration::from_millis(100),
            event_count: 0,
            has_been_active: false,
        }
    }

    #[test]
    fn grace_period_blocks_immediate_deactivation() {
        // 模拟 Wayland 焦点抖动：窗口刚创建就收到 active→inactive
        let mut tracker = PopupActivationTracker::default();

        assert_eq!(
            tracker.on_activation_event(true, true),
            PopupActivationDecision::KeepOpen
        ); // 获得焦点
        assert!(matches!(
            tracker.on_activation_event(false, true),
            PopupActivationDecision::RecheckAfter(_)
        )); // 立即失焦——在保护期内，不关闭
    }

    #[test]
    fn auto_hide_requires_popup_to_have_been_active_first() {
        let mut tracker = tracker_past_grace();

        assert_eq!(
            tracker.on_activation_event(false, true),
            PopupActivationDecision::KeepOpen
        );
        assert_eq!(
            tracker.on_activation_event(false, true),
            PopupActivationDecision::KeepOpen
        );
    }

    #[test]
    fn auto_hide_closes_after_popup_loses_focus_post_activation() {
        let mut tracker = tracker_past_grace();

        assert_eq!(
            tracker.on_activation_event(true, true),
            PopupActivationDecision::KeepOpen
        );
        assert_eq!(
            tracker.on_activation_event(false, true),
            PopupActivationDecision::Close
        );
    }

    #[test]
    fn auto_hide_closes_after_late_activation_then_blur() {
        let mut tracker = tracker_past_grace();

        assert_eq!(
            tracker.on_activation_event(false, true),
            PopupActivationDecision::KeepOpen
        );
        assert_eq!(
            tracker.on_activation_event(true, true),
            PopupActivationDecision::KeepOpen
        );
        assert_eq!(
            tracker.on_activation_event(false, true),
            PopupActivationDecision::Close
        );
    }

    #[test]
    fn auto_hide_respects_setting_after_activation() {
        let mut tracker = tracker_past_grace();

        assert_eq!(
            tracker.on_activation_event(true, false),
            PopupActivationDecision::KeepOpen
        );
        assert_eq!(
            tracker.on_activation_event(false, false),
            PopupActivationDecision::KeepOpen
        );
    }
}
