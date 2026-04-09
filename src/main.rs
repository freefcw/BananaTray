#![recursion_limit = "512"]

rust_i18n::i18n!("locales", fallback = "en");

mod app;
mod app_state;
mod application;
mod assets;
mod auto_launch;
mod i18n;
mod logging;
pub mod models;
pub mod notification;
mod provider_error_presenter;
mod providers;
mod refresh;
mod runtime;
mod settings_store;
mod single_instance;
mod theme;
mod tray_icon_helper;
mod utils;

use app::{schedule_open_settings_window, AppState};
use application::AppAction;
use assets::Assets;
use gpui::*;
use log::{debug, error, info};
use models::NavTab;
use refresh::{RefreshCoordinator, RefreshReason, RefreshRequest};
use rust_i18n::t;
use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;

// ============================================================================
// macOS: 多显示器感知的托盘弹窗定位
// ============================================================================
//
// GPUI 的 tray_icon_bounds() 内部用 NSScreen::mainScreen（焦点屏幕）做 Y 翻转，
// 但 MacWindow::open() 用 primary screen 高度做反向转换。当两者高度不同时产生偏差。
// 此模块绕过该链路，直接用 CoreGraphics 鼠标坐标计算 display-local 位置。
// ============================================================================

#[cfg(target_os = "macos")]
mod tray_display {
    use gpui::{point, px, App, Bounds, DisplayId, Pixels, Size};

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
    type CGEventRef = *const std::ffi::c_void;

    extern "C" {
        fn CGEventCreate(source: *const std::ffi::c_void) -> CGEventRef;
        fn CGEventGetLocation(event: CGEventRef) -> CGPoint;
        fn CFRelease(cf: *const std::ffi::c_void);
        fn CGDisplayBounds(display: CGDirectDisplayID) -> CGRect;
    }

    /// 获取鼠标光标的全局位置（CoreGraphics 坐标系：主屏幕左上角为原点，Y 向下）
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

    /// 计算托盘弹窗在多显示器环境中的正确位置。
    ///
    /// 通过 CoreGraphics 获取鼠标位置（位于托盘图标内），找到对应显示器，
    /// 然后计算 display-local 坐标，避免 GPUI 内部 mainScreen/primaryScreen 混用导致的偏差。
    ///
    /// 返回 (display-local bounds, target display_id)。
    pub fn compute_tray_popup_bounds(
        cx: &App,
        window_size: Size<Pixels>,
        tray_bounds: Bounds<Pixels>,
    ) -> (Bounds<Pixels>, Option<DisplayId>) {
        let Some(mouse) = mouse_position() else {
            log::warn!(target: "tray", "无法获取鼠标位置，回退到默认定位");
            return (fallback_bounds(window_size, tray_bounds), None);
        };

        // 在所有显示器中找到鼠标所在的那个
        let displays = cx.displays();
        let target = displays.iter().find_map(|d| {
            let id_u32: u32 = d.id().into();
            let rect = unsafe { CGDisplayBounds(id_u32) };
            let contains = mouse.x >= rect.origin.x
                && mouse.x < rect.origin.x + rect.size.width
                && mouse.y >= rect.origin.y
                && mouse.y < rect.origin.y + rect.size.height;
            if contains {
                Some((d.id(), rect))
            } else {
                None
            }
        });

        let Some((display_id, display_rect)) = target else {
            log::warn!(target: "tray", "未找到鼠标所在显示器，回退到默认定位");
            return (fallback_bounds(window_size, tray_bounds), None);
        };

        // 托盘图标的全局 x 坐标（macOS 和 CG 的 x 轴方向一致）
        let tray_center_x = tray_bounds.origin.x + tray_bounds.size.width * 0.5;
        // 转为 display-local x 并居中窗口
        let local_x = tray_center_x - px(display_rect.origin.x as f32) - window_size.width * 0.5;

        // 鼠标 Y 坐标（CG 坐标，相对于主屏左上角）转为 display-local
        // 用户在菜单栏点击托盘图标时，鼠标 Y ≈ 菜单栏高度（约 25pt）
        let mouse_local_y = px((mouse.y - display_rect.origin.y) as f32);
        // 取鼠标 Y 和托盘图标高度中的较大值，确保窗口在菜单栏下方
        let local_y = mouse_local_y.max(tray_bounds.size.height);

        // 确保窗口不超出屏幕左右边界
        let display_width = px(display_rect.size.width as f32);
        let clamped_x = local_x.max(px(0.0)).min(display_width - window_size.width);

        let bounds = Bounds::new(point(clamped_x, local_y), window_size);

        log::debug!(
            target: "tray",
            "multi-display positioning: mouse=({:.0},{:.0}), display={:?} rect=({:.0},{:.0} {:.0}x{:.0}), result=({:.1},{:.1})",
            mouse.x, mouse.y,
            display_id,
            display_rect.origin.x, display_rect.origin.y,
            display_rect.size.width, display_rect.size.height,
            clamped_x, local_y,
        );

        (bounds, Some(display_id))
    }

    /// 回退定位：直接使用 tray bounds（仅在无法获取鼠标位置时使用）
    fn fallback_bounds(window_size: Size<Pixels>, tray: Bounds<Pixels>) -> Bounds<Pixels> {
        let x = tray.origin.x + (tray.size.width - window_size.width) * 0.5;
        let y = tray.origin.y + tray.size.height;
        Bounds::new(point(x, y), window_size)
    }
}

// ============================================================================
// TrayController — 窗口管理器
// ============================================================================

/// 窗口管理器：持有全局窗口句柄，纯数据，不含任何锁操作
struct TrayController {
    window: Option<WindowHandle<app::AppView>>,
    state: Rc<RefCell<AppState>>,
}

impl TrayController {
    fn new(
        refresh_tx: smol::channel::Sender<RefreshRequest>,
        manager: &crate::providers::ProviderManager,
        log_path: Option<std::path::PathBuf>,
    ) -> Self {
        info!(target: "tray", "initializing tray controller");
        let state = Rc::new(RefCell::new(AppState::new(refresh_tx, manager, log_path)));
        Self {
            window: None,
            state,
        }
    }

    /// Close the tray popup window and clear the view entity reference.
    /// Returns the display ID the popup was on, if available.
    fn close_popup(&mut self, cx: &mut App) -> Option<DisplayId> {
        let window = self.window.take()?;
        self.state.borrow_mut().view_entity = None;
        let mut display_id = None;
        let _ = window.update(cx, |_, window, cx| {
            display_id = window.display(cx).map(|d| d.id());
            window.remove_window();
        });
        // 弹窗关闭后同步动态图标
        runtime::dispatch_in_app(&self.state, AppAction::PopupVisibilityChanged(false), cx);
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

    fn toggle_provider(&mut self, cx: &mut App) {
        let provider_tab = {
            let mut state = self.state.borrow_mut();
            state.session.default_provider_tab()
        };

        let Some(provider_tab) = provider_tab else {
            info!(target: "tray", "no providers enabled, opening settings directly");
            self.show_settings(cx);
            return;
        };
        info!(target: "tray", "toggle provider panel for {:?}", provider_tab);

        // Check if window is actually alive, not just if handle exists
        if self.is_window_alive(cx) {
            let active_tab = self.state.borrow().session.nav.active_tab.clone();
            if matches!(active_tab, NavTab::Provider(_)) {
                info!(target: "tray", "provider panel already open, closing existing panel");
                self.close_popup(cx);
            } else {
                info!(target: "tray", "reusing existing window handle for provider panel");
                self.show(provider_tab, cx);
            }
        } else {
            // Handle is stale, clear it
            info!(target: "tray", "window handle is stale, clearing and opening fresh panel");
            self.window = None;
            self.show(provider_tab, cx);
        }
    }

    fn show_settings(&mut self, cx: &mut App) {
        info!(target: "tray", "requested settings window from tray controller");
        let display_id = self.close_popup(cx);
        schedule_open_settings_window(self.state.clone(), display_id, cx);
    }

    fn show(&mut self, tab: NavTab, cx: &mut App) {
        info!(target: "tray", "show window for tab {:?}", tab);
        runtime::dispatch_in_app(&self.state, AppAction::SelectNavTab(tab), cx);

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
                return tray_display::compute_tray_popup_bounds(cx, window_size, tray_bounds);
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
        let window_size = size(px(models::PopupLayout::WIDTH), px(dynamic_height));
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
            |_window, cx| cx.new(|cx| app::AppView::new(state, cx)),
        );

        if let Ok(handle) = result {
            info!(target: "tray", "tray popup opened successfully");
            // 标记弹窗可见
            runtime::dispatch_in_app(&self.state, AppAction::PopupVisibilityChanged(true), cx);
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

// ============================================================================
// Bootstrap — 应用初始化
// ============================================================================

/// 初始化 i18n、UI 工具包、托盘图标（在 GPUI run 闭包内调用）
fn bootstrap_ui(cx: &mut App) {
    // i18n locale
    let settings = crate::settings_store::load().unwrap_or_default();
    crate::i18n::apply_locale(&settings.display.language);

    // adabraka-ui 工具包
    adabraka_ui::init(cx);
    adabraka_ui::theme::install_theme(cx, adabraka_ui::theme::Theme::light());
    cx.set_keep_alive_without_windows(true);

    // 系统托盘
    let icon_request = match settings.display.tray_icon_style {
        crate::models::TrayIconStyle::Dynamic => {
            // 启动时数据尚未加载，默认 Green（= Monochrome），首次刷新后会自动更新
            crate::application::TrayIconRequest::DynamicStatus(crate::models::StatusLevel::Green)
        }
        style => crate::application::TrayIconRequest::Static(style),
    };
    crate::tray_icon_helper::apply_tray_icon(cx, icon_request);
    cx.set_tray_tooltip(&t!("tray.tooltip"));
    cx.set_tray_panel_mode(true);

    // 通知授权（仅在 App Bundle 模式下请求）
    crate::notification::request_notification_authorization();
}

/// 创建 ProviderManager + RefreshCoordinator，启动后台刷新线程。
/// 返回 (refresh_tx, event_rx, manager) 供后续步骤使用。
fn bootstrap_refresh() -> (
    smol::channel::Sender<RefreshRequest>,
    smol::channel::Receiver<refresh::RefreshEvent>,
    std::sync::Arc<crate::providers::ProviderManager>,
) {
    let (event_tx, event_rx) = smol::channel::bounded::<refresh::RefreshEvent>(64);
    let manager = std::sync::Arc::new(crate::providers::ProviderManager::new());
    let coordinator = RefreshCoordinator::new(manager.clone(), event_tx);
    let refresh_tx = coordinator.sender();

    std::thread::Builder::new()
        .name("refresh-coordinator".into())
        .spawn(move || smol::block_on(coordinator.run()))
        .expect("failed to spawn refresh coordinator thread");

    (refresh_tx, event_rx, manager)
}

/// 启动事件泵：从协调器接收 RefreshEvent，分派到 UI 线程更新 AppState
fn start_event_pump(
    state: &Rc<RefCell<AppState>>,
    event_rx: smol::channel::Receiver<refresh::RefreshEvent>,
    cx: &mut App,
) {
    let state = state.clone();
    let pump_cx = cx.to_async();
    cx.to_async()
        .foreground_executor()
        .spawn(async move {
            while let Ok(event) = event_rx.recv().await {
                let _ = pump_cx.update(|cx| {
                    runtime::dispatch_in_app(&state, AppAction::RefreshEventReceived(event), cx);
                });
            }
        })
        .detach();
}

/// 发送初始配置同步 + 启动首次刷新
fn trigger_initial_refresh(state: &Rc<RefCell<AppState>>) {
    let config_request = crate::application::build_config_sync_request(&state.borrow().session);
    let _ = state.borrow().send_refresh(config_request);
    let _ = state.borrow().send_refresh(RefreshRequest::RefreshAll {
        reason: RefreshReason::Startup,
    });
}

/// 注册托盘图标事件（左键/右键）
fn register_tray_events(controller: &Rc<RefCell<TrayController>>, cx: &mut App) {
    let ctrl = controller.clone();
    cx.on_tray_icon_event(move |event, cx| {
        info!(target: "tray", "received tray event: {:?}", event);
        match event {
            TrayIconEvent::LeftClick => ctrl.borrow_mut().toggle_provider(cx),
            TrayIconEvent::RightClick => ctrl.borrow_mut().show_settings(cx),
            _ => {}
        }
    });
}

/// 注册全局热键 Cmd+Shift+S
fn register_global_hotkey(controller: &Rc<RefCell<TrayController>>, cx: &mut App) {
    info!(target: "hotkey", "registering global hotkey Cmd+Shift+S");
    if let Ok(keystroke) = Keystroke::parse("cmd-shift-s") {
        let _ = cx.register_global_hotkey(1, &keystroke);
    }
    let async_cx = cx.to_async();
    let ctrl = controller.clone();
    cx.on_global_hotkey(move |id| {
        if id == 1 {
            info!(target: "hotkey", "received global hotkey 1");
            let _ = async_cx.update(|cx| {
                ctrl.borrow_mut().toggle_provider(cx);
            });
        }
    });
}

/// 监听二次实例的 SHOW 请求，桥接 std::sync::mpsc → 前台 executor
fn listen_for_secondary_instance(
    controller: &Rc<RefCell<TrayController>>,
    show_rx: std::sync::mpsc::Receiver<()>,
    cx: &mut App,
) {
    let (show_async_tx, show_async_rx) = smol::channel::bounded::<()>(4);
    std::thread::Builder::new()
        .name("single-instance-bridge".into())
        .spawn(move || {
            while show_rx.recv().is_ok() {
                if show_async_tx.send_blocking(()).is_err() {
                    break;
                }
            }
        })
        .expect("failed to spawn single-instance bridge thread");

    let ctrl = controller.clone();
    let show_async_cx = cx.to_async();
    cx.to_async()
        .foreground_executor()
        .spawn(async move {
            while show_async_rx.recv().await.is_ok() {
                info!(target: "app", "secondary instance requested SHOW");
                let _ = show_async_cx.update(|cx| {
                    ctrl.borrow_mut().toggle_provider(cx);
                });
            }
        })
        .detach();
}

// ============================================================================
// Entry Point
// ============================================================================

fn main() {
    if try_run_codeium_family_debug_cli() {
        return;
    }

    let log_path = match logging::init() {
        Ok(init) => {
            log::info!(target: "app", "logging initialized at {}", init.log_path.display());
            Some(init.log_path)
        }
        Err(err) => {
            eprintln!("failed to initialize logging: {err:#}");
            None
        }
    };

    // Single-instance check: must run before Application::new() so that a
    // secondary process exits immediately without initializing the UI toolkit.
    let show_rx = match single_instance::ensure_single_instance() {
        single_instance::InstanceRole::Primary(rx) => rx,
        single_instance::InstanceRole::Secondary => {
            info!(target: "app", "another instance is already running, exiting");
            std::process::exit(0);
        }
    };

    Application::new()
        .with_assets(Assets::new())
        .run(move |cx: &mut App| {
            // 1. UI + 托盘初始化
            bootstrap_ui(cx);

            // 2. 后台刷新系统
            let (refresh_tx, event_rx, manager) = bootstrap_refresh();

            // 3. 窗口控制器
            let controller = Rc::new(RefCell::new(TrayController::new(
                refresh_tx,
                &manager,
                log_path.clone(),
            )));

            // 4. 事件泵
            start_event_pump(&controller.borrow().state, event_rx, cx);

            // 5. 初始刷新
            trigger_initial_refresh(&controller.borrow().state);

            // 6. 注册事件处理器
            register_tray_events(&controller, cx);
            register_global_hotkey(&controller, cx);
            listen_for_secondary_instance(&controller, show_rx, cx);

            info!(target: "app", "BananaTray is running - look for the tray icon");
        });
}

fn try_run_codeium_family_debug_cli() -> bool {
    let mut args = std::env::args().skip(1);
    let Some(first) = args.next() else {
        return false;
    };

    if first != "debug-codeium-family" {
        return false;
    }

    let selector = args.next();
    match crate::providers::codeium_family::debug_report(selector.as_deref()) {
        Ok(report) => {
            println!("{}", report);
            std::process::exit(0);
        }
        Err(err) => {
            eprintln!("debug-codeium-family failed: {err:#}");
            eprintln!("usage: bananatray debug-codeium-family [antigravity|windsurf|all]");
            std::process::exit(2);
        }
    }
}
