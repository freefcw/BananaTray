//! 应用初始化 — 启动时调用一次的设置和注册函数。
//!
//! `bootstrap` 是 shell composition root：这里组合具体 GPUI window、tray、
//! D-Bus handle 和 App/Window 级 dispatch facade；`runtime` 只接收能力抽象。

use crate::application::{AppAction, GlobalHotkeyError};
use crate::models::{AppSettings, ScriptProviderConfig, SystemSettings};
use crate::refresh::{RefreshCoordinator, RefreshReason, RefreshRequest};
use crate::runtime::AppState;
use crate::tray::TrayController;
use gpui::{
    point, px, size, App, Bounds, DisplayId, TrayIconClickEvent, Window, WindowBounds,
    WindowHandle, WindowKind, WindowOptions,
};
use log::{info, warn};
use rust_i18n::t;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

type ScriptTestRequest = (u64, ScriptProviderConfig);
type ScriptTestSender = smol::channel::Sender<ScriptTestRequest>;
type ScriptTestReceiver = smol::channel::Receiver<ScriptTestRequest>;

type NotifyPopupViewFn = fn(&Rc<RefCell<AppState>>, &mut App);
type BuildSettingsViewFn =
    fn(Rc<RefCell<AppState>>, &mut App) -> gpui::Entity<crate::ui::settings_window::SettingsView>;
type ClearPopupViewFn = fn(&Rc<RefCell<AppState>>);

thread_local! {
    static NOTIFY_POPUP_VIEW_FN: RefCell<Option<NotifyPopupViewFn>> = const { RefCell::new(None) };
    static BUILD_SETTINGS_VIEW_FN: RefCell<Option<BuildSettingsViewFn>> = const { RefCell::new(None) };
    static CLEAR_POPUP_VIEW_FN: RefCell<Option<ClearPopupViewFn>> = const { RefCell::new(None) };
    static SETTINGS_WINDOW: RefCell<Option<WindowHandle<crate::ui::settings_window::SettingsView>>> = const { RefCell::new(None) };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrayCommand {
    ToggleProvider,
    ShowSettings,
    #[cfg(target_os = "linux")]
    Quit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum StartupHotkeyRegistration {
    Registered {
        persisted: String,
        canonicalized: bool,
    },
    RecoverWithDefault,
    KeepConfiguredError(GlobalHotkeyError),
}

pub(crate) fn load_settings() -> AppSettings {
    crate::settings_store::load().unwrap_or_else(|err| {
        warn!(target: "settings", "failed to load saved settings: {err}");
        AppSettings::default()
    })
}

pub(crate) fn sync_initial_auto_launch(settings: &AppSettings) {
    crate::platform::auto_launch::sync(settings.system.start_at_login);
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

pub(crate) fn notify_popup_view(state: &Rc<RefCell<AppState>>, cx: &mut App) {
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

pub(crate) fn build_settings_view(
    state: Rc<RefCell<AppState>>,
    cx: &mut App,
) -> Option<gpui::Entity<crate::ui::settings_window::SettingsView>> {
    BUILD_SETTINGS_VIEW_FN.with(|slot| slot.borrow().map(|f| f(state, cx)))
}

pub(crate) fn apply_tray_icon(cx: &mut App, request: crate::application::TrayIconRequest) {
    crate::tray::apply_tray_icon(cx, request);
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

    let existing_handle = SETTINGS_WINDOW.with(|slot| *slot.borrow());
    let activated_existing = if let Some(handle) = existing_handle {
        info!(
            target: "settings",
            "existing settings window found, attempting to activate it"
        );
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
                info!(
                    target: "settings",
                    "window on different display, closing to reopen on target display"
                );
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

    #[cfg(debug_assertions)]
    SETTINGS_WINDOW.with(|slot| {
        debug_assert!(
            slot.borrow().is_none(),
            "stale settings window slot before opening a new window"
        );
    });

    let settings_state = state.clone();
    let window_size = size(px(600.0), px(640.0));
    let display_bounds = target_display_id
        .and_then(|id| cx.find_display(id))
        .or_else(|| cx.primary_display())
        .map(|d| d.bounds());
    let origin = match display_bounds {
        Some(db) => point(
            db.origin.x + (db.size.width - window_size.width) / 2.0,
            db.origin.y + (db.size.height - window_size.height) / 2.0,
        ),
        None => point(px(0.0), px(0.0)),
    };
    let window_bounds = WindowBounds::Windowed(Bounds {
        origin,
        size: window_size,
    });

    let Some(build_view) = build_settings_view(settings_state, cx) else {
        log::error!(target: "settings", "settings view factory not registered");
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
                log::debug!(
                    target: "settings",
                    "system appearance changed, settings window refreshed"
                );
            });
            view._appearance_sub = Some(appearance_sub);
        });
        info!(target: "settings", "requested app/window activation for settings window");
        SETTINGS_WINDOW.with(|slot| {
            *slot.borrow_mut() = Some(handle);
        });
    } else if let Err(err) = result {
        log::error!(target: "settings", "failed to open settings window: {err:?}");
    }
}

struct WindowShellCaps<'a> {
    window: &'a mut Window,
    cx: &'a mut App,
}

impl crate::runtime::ContextCapabilities for WindowShellCaps<'_> {
    fn render(&mut self, _state: &Rc<RefCell<AppState>>) {
        self.window.refresh();
    }
}

impl crate::runtime::FullContextCapabilities for WindowShellCaps<'_> {
    fn open_settings_window(&mut self, state: &Rc<RefCell<AppState>>) {
        let display_id = self.window.display(self.cx).map(|display| display.id());
        clear_popup_view(state);
        self.window.remove_window();
        schedule_open_settings_window(state.clone(), display_id, self.cx);
    }

    fn apply_tray_icon(&mut self, request: crate::application::TrayIconRequest) {
        apply_tray_icon(self.cx, request);
    }

    fn apply_global_hotkey(&mut self, state: &Rc<RefCell<AppState>>, hotkey: &str) {
        crate::runtime::global_hotkey::rebind_global_hotkey(state, hotkey, self.cx);
    }

    fn quit(&mut self) {
        self.cx.quit();
    }
}

struct AppShellCaps<'a> {
    cx: &'a mut App,
}

impl crate::runtime::ContextCapabilities for AppShellCaps<'_> {
    fn render(&mut self, state: &Rc<RefCell<AppState>>) {
        notify_popup_view(state, self.cx);
    }
}

impl crate::runtime::FullContextCapabilities for AppShellCaps<'_> {
    fn open_settings_window(&mut self, state: &Rc<RefCell<AppState>>) {
        schedule_open_settings_window(state.clone(), None, self.cx);
    }

    fn apply_tray_icon(&mut self, request: crate::application::TrayIconRequest) {
        apply_tray_icon(self.cx, request);
    }

    fn apply_global_hotkey(&mut self, state: &Rc<RefCell<AppState>>, hotkey: &str) {
        crate::runtime::global_hotkey::rebind_global_hotkey(state, hotkey, self.cx);
    }

    fn quit(&mut self) {
        self.cx.quit();
    }
}

pub(crate) fn dispatch_in_window(
    state: &Rc<RefCell<AppState>>,
    action: AppAction,
    window: &mut Window,
    cx: &mut App,
) {
    crate::runtime::dispatch_with_full_context(state, action, &mut WindowShellCaps { window, cx });
}

pub(crate) fn dispatch_in_app(state: &Rc<RefCell<AppState>>, action: AppAction, cx: &mut App) {
    crate::runtime::dispatch_with_full_context(state, action, &mut AppShellCaps { cx });
}

/// 初始化 i18n、UI 工具包、托盘图标（在 GPUI run 闭包内调用）
pub(crate) fn bootstrap_ui(cx: &mut App, settings: &AppSettings) {
    // i18n locale
    crate::i18n::apply_locale(&settings.display.language);
    crate::ui::register_shell_hooks();

    // adabraka-ui 工具包
    adabraka_ui::init(cx);
    adabraka_ui::theme::install_theme(cx, adabraka_ui::theme::Theme::light());
    cx.set_keep_alive_without_windows(true);
    crate::runtime::register_idle_gpu_cache_trim(cx);

    if crate::tray::should_use_gpui_tray() {
        // 系统托盘
        let icon_request = match settings.display.tray_icon_style {
            crate::models::TrayIconStyle::Dynamic => {
                // 启动时数据尚未加载，默认 Green（= Monochrome），首次刷新后会自动更新
                crate::application::TrayIconRequest::DynamicStatus(
                    crate::models::StatusLevel::Green,
                )
            }
            style => crate::application::TrayIconRequest::Static(style),
        };
        apply_tray_icon(cx, icon_request);
        cx.set_tray_tooltip(&t!("tray.tooltip"));
        #[cfg(target_os = "macos")]
        {
            // macOS status item defaults to NSMenu mode; panel mode is required for
            // clicks to reach `on_tray_icon_event` and toggle the GPUI popup.
            cx.set_tray_panel_mode(true);
        }
    } else {
        info!(target: "tray", "GNOME extension mode detected, skipping GPUI tray bootstrap");
    }

    // 通知授权（仅在 App Bundle 模式下请求）
    crate::platform::notification::request_notification_authorization();
}

fn command_for_tray_icon_event(event: &TrayIconClickEvent) -> Option<TrayCommand> {
    use gpui::TrayIconEvent;
    match &event.kind {
        TrayIconEvent::LeftClick => Some(TrayCommand::ToggleProvider),
        TrayIconEvent::RightClick => Some(TrayCommand::ShowSettings),
        _ => None,
    }
}

fn run_tray_command(command: TrayCommand, controller: &Rc<RefCell<TrayController>>, cx: &mut App) {
    match command {
        TrayCommand::ToggleProvider => controller.borrow_mut().toggle_provider(cx),
        TrayCommand::ShowSettings => controller.borrow_mut().show_settings(cx),
        #[cfg(target_os = "linux")]
        TrayCommand::Quit => cx.quit(),
    }
}

/// 创建 ProviderManager + RefreshCoordinator，启动后台刷新线程。
/// 返回 (refresh_tx, event_rx, manager) 供后续步骤使用。
pub(crate) fn bootstrap_refresh() -> (
    smol::channel::Sender<RefreshRequest>,
    smol::channel::Receiver<crate::refresh::RefreshEvent>,
    crate::providers::ProviderManagerHandle,
) {
    let (event_tx, event_rx) = smol::channel::bounded::<crate::refresh::RefreshEvent>(64);
    let manager = crate::providers::ProviderManagerHandle::default();
    let coordinator = RefreshCoordinator::new(manager.clone(), event_tx);
    let refresh_tx = coordinator.sender();

    std::thread::Builder::new()
        .name("refresh-coordinator".into())
        .spawn(move || smol::block_on(coordinator.run()))
        .expect("failed to spawn refresh coordinator thread");

    (refresh_tx, event_rx, manager)
}

pub(crate) fn script_test_channel() -> (ScriptTestSender, ScriptTestReceiver) {
    smol::channel::bounded(8)
}

/// 启动事件泵：从协调器接收 RefreshEvent，分派到 UI 线程更新 AppState
#[cfg(target_os = "linux")]
pub(crate) fn start_event_pump(
    state: &Rc<RefCell<AppState>>,
    event_rx: smol::channel::Receiver<crate::refresh::RefreshEvent>,
    dbus_handle: Option<crate::dbus::DBusServiceHandle>,
    cx: &mut App,
) {
    let state = state.clone();
    let pump_cx = cx.to_async();
    cx.to_async()
        .foreground_executor()
        .spawn(async move {
            while let Ok(event) = event_rx.recv().await {
                let _ = pump_cx.update(|cx| {
                    dispatch_in_app(&state, AppAction::RefreshEventReceived(event), cx);

                    // Linux: D-Bus 信号发射（reducer 已更新 AppState）
                    #[cfg(target_os = "linux")]
                    emit_current_dbus_snapshot(&state, dbus_handle.as_ref());
                });
            }
        })
        .detach();
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn start_event_pump(
    state: &Rc<RefCell<AppState>>,
    event_rx: smol::channel::Receiver<crate::refresh::RefreshEvent>,
    cx: &mut App,
) {
    let state = state.clone();
    let pump_cx = cx.to_async();
    cx.to_async()
        .foreground_executor()
        .spawn(async move {
            while let Ok(event) = event_rx.recv().await {
                let _ = pump_cx.update(|cx| {
                    dispatch_in_app(&state, AppAction::RefreshEventReceived(event), cx);
                });
            }
        })
        .detach();
}

/// 启动脚本测试事件泵：后台执行脚本，完成后回到 UI 线程回填结果。
pub(crate) fn start_script_test_pump(
    state: &Rc<RefCell<AppState>>,
    script_test_rx: ScriptTestReceiver,
    cx: &mut App,
) {
    let (result_tx, result_rx) =
        smol::channel::bounded::<(u64, crate::models::ScriptProviderTestResult)>(8);

    std::thread::Builder::new()
        .name("script-provider-test".into())
        .spawn(move || {
            while let Ok((request_id, config)) = smol::block_on(script_test_rx.recv()) {
                let result = crate::runtime::execute_script_provider_test(&config);
                if smol::block_on(result_tx.send((request_id, result))).is_err() {
                    break;
                }
            }
        })
        .expect("failed to spawn script provider test thread");

    let state = state.clone();
    let pump_cx = cx.to_async();
    cx.to_async()
        .foreground_executor()
        .spawn(async move {
            while let Ok((request_id, result)) = result_rx.recv().await {
                let _ = pump_cx.update(|cx| {
                    dispatch_in_app(
                        &state,
                        AppAction::ScriptProviderTestFinished { request_id, result },
                        cx,
                    );
                });
            }
        })
        .detach();
}

/// 向 GNOME Shell Extension 发射当前状态快照。
#[cfg(target_os = "linux")]
pub(crate) fn emit_current_dbus_snapshot(
    state: &Rc<RefCell<AppState>>,
    handle: Option<&crate::dbus::DBusServiceHandle>,
) {
    use crate::application::DBusQuotaSnapshot;

    if let Some(handle) = handle {
        let state_ref = state.borrow();
        let snapshot = DBusQuotaSnapshot::from_session(&state_ref.session);
        match serde_json::to_string(&snapshot) {
            Ok(json) => {
                if let Err(e) = handle.emit_refresh_complete(json) {
                    warn!(target: "dbus", "failed to emit RefreshComplete: {e}");
                }
            }
            Err(e) => {
                warn!(target: "dbus", "failed to serialize D-Bus snapshot: {e}");
            }
        }
    }
}

/// 发送初始配置同步 + 启动首次刷新
pub(crate) fn trigger_initial_refresh(state: &Rc<RefCell<AppState>>) {
    let config_request = crate::application::build_config_sync_request(&state.borrow().session);
    if let Err(e) = state.borrow().send_refresh(config_request) {
        warn!(target: "app", "failed to send initial config sync: {e}");
    }
    if let Err(e) = state.borrow().send_refresh(RefreshRequest::RefreshAll {
        reason: RefreshReason::Startup,
    }) {
        warn!(target: "app", "failed to send initial refresh: {e}");
    }
}

/// 注册托盘图标事件（左键/右键）和 Linux 菜单
pub(crate) fn register_tray_events(controller: &Rc<RefCell<TrayController>>, cx: &mut App) {
    #[cfg(target_os = "linux")]
    if !crate::tray::should_use_gpui_tray() {
        info!(target: "tray", "GNOME extension mode detected, skipping GPUI tray event setup");
        return;
    }

    let ctrl = controller.clone();
    cx.on_tray_icon_click_event(move |event, cx| {
        info!(target: "tray", "received tray click event: {:?} position={:?}", event.kind, event.position);
        // 将点击坐标传递给 controller，用于 Linux 上构造 TrayAnchor
        ctrl.borrow().set_click_position(event.position);
        if let Some(command) = command_for_tray_icon_event(&event) {
            run_tray_command(command, &ctrl, cx);
        }
    });

    // Linux: 注册右键菜单和菜单动作回调
    // GNOME AppIndicator 扩展行为：单击 → 菜单，双击 → Activate（打开窗口）
    // GNOME Shell Extension 模式下跳过菜单安装，由扩展处理交互
    #[cfg(target_os = "linux")]
    {
        install_linux_tray_menu(cx);
        let ctrl = controller.clone();
        cx.on_tray_menu_action(move |id, cx| {
            info!(target: "tray", "received tray menu action: {}", id);
            if let Some(command) = command_for_tray_menu_action(&id) {
                run_tray_command(command, &ctrl, cx);
            }
        });
    }
}

#[cfg(target_os = "linux")]
const TRAY_ACTION_OPEN: &str = "tray.open";
#[cfg(target_os = "linux")]
const TRAY_ACTION_SETTINGS: &str = "tray.settings";
#[cfg(target_os = "linux")]
const TRAY_ACTION_QUIT: &str = "tray.quit";

#[cfg(target_os = "linux")]
fn install_linux_tray_menu(cx: &mut App) {
    use gpui::TrayMenuItem;

    cx.set_tray_menu(vec![
        TrayMenuItem::Action {
            label: t!("tray.menu.open").to_string().into(),
            id: TRAY_ACTION_OPEN.into(),
        },
        TrayMenuItem::Action {
            label: t!("tray.menu.settings").to_string().into(),
            id: TRAY_ACTION_SETTINGS.into(),
        },
        TrayMenuItem::Separator,
        TrayMenuItem::Action {
            label: t!("tray.menu.quit").to_string().into(),
            id: TRAY_ACTION_QUIT.into(),
        },
    ]);
}

#[cfg(target_os = "linux")]
fn command_for_tray_menu_action(id: &str) -> Option<TrayCommand> {
    match id {
        TRAY_ACTION_OPEN => Some(TrayCommand::ToggleProvider),
        TRAY_ACTION_SETTINGS => Some(TrayCommand::ShowSettings),
        TRAY_ACTION_QUIT => Some(TrayCommand::Quit),
        _ => None,
    }
}

/// 注册全局热键（从 settings 读取，可在运行时重新绑定）
pub(crate) fn register_global_hotkey(controller: &Rc<RefCell<TrayController>>, cx: &mut App) {
    let state = controller.borrow().state();
    let configured_hotkey = state.borrow().session.settings.system.global_hotkey.clone();

    match classify_startup_hotkey_registration(
        &configured_hotkey,
        crate::runtime::global_hotkey::register_hotkey_string(&configured_hotkey, None, cx),
    ) {
        StartupHotkeyRegistration::Registered {
            persisted,
            canonicalized,
        } => {
            clear_global_hotkey_error(&state);

            if canonicalized {
                persist_hotkey_value(&state, persisted, "startup canonicalization");
            }
        }
        StartupHotkeyRegistration::RecoverWithDefault => {
            warn!(
                target: "settings",
                "configured global hotkey {} is invalid; falling back to default {}",
                configured_hotkey,
                SystemSettings::DEFAULT_GLOBAL_HOTKEY
            );

            let fallback_hotkey = SystemSettings::DEFAULT_GLOBAL_HOTKEY.to_string();
            persist_hotkey_value(&state, fallback_hotkey.clone(), "startup recovery");

            match crate::runtime::global_hotkey::register_hotkey_string(
                SystemSettings::DEFAULT_GLOBAL_HOTKEY,
                None,
                cx,
            ) {
                Ok(_) => {
                    clear_global_hotkey_error(&state);
                }
                Err(fallback_err) => {
                    warn!(
                        target: "settings",
                        "failed to register fallback global hotkey {}: {:?}",
                        SystemSettings::DEFAULT_GLOBAL_HOTKEY,
                        fallback_err
                    );
                    set_global_hotkey_error(&state, fallback_hotkey, fallback_err);
                }
            }
        }
        StartupHotkeyRegistration::KeepConfiguredError(err) => {
            let error_hotkey = normalize_hotkey_error_candidate(&configured_hotkey)
                .unwrap_or(configured_hotkey.clone());
            warn!(
                target: "settings",
                "failed to register configured global hotkey {}: {:?}; keeping saved value",
                configured_hotkey,
                err
            );
            set_global_hotkey_error(&state, error_hotkey, err);
        }
    }

    let async_cx = cx.to_async();
    let ctrl = controller.clone();
    cx.on_global_hotkey(move |id| {
        if id == crate::runtime::global_hotkey::GLOBAL_HOTKEY_ID {
            info!(target: "app", "received global hotkey {}", id);
            let _ = async_cx.update(|cx| {
                ctrl.borrow_mut().toggle_provider(cx);
            });
        }
    });
}

fn classify_startup_hotkey_registration(
    configured_hotkey: &str,
    result: Result<String, GlobalHotkeyError>,
) -> StartupHotkeyRegistration {
    match result {
        Ok(persisted) => StartupHotkeyRegistration::Registered {
            canonicalized: persisted != configured_hotkey,
            persisted,
        },
        Err(err) if err.is_invalid_configuration() => StartupHotkeyRegistration::RecoverWithDefault,
        Err(err) => StartupHotkeyRegistration::KeepConfiguredError(err),
    }
}

fn normalize_hotkey_error_candidate(hotkey: &str) -> Option<String> {
    crate::runtime::global_hotkey::parse_hotkey_string(hotkey)
        .map(|keystroke| crate::runtime::global_hotkey::format_hotkey_for_settings(&keystroke))
        .ok()
}

// BYPASS: bootstrap-only direct state mutation, too simple for full Action-Reducer-Effect cycle.
// If bootstrap state operations grow beyond 5 call sites, migrate to dispatch pipeline.
fn clear_global_hotkey_error(state: &Rc<RefCell<AppState>>) {
    let mut s = state.borrow_mut();
    s.session.settings_ui.global_hotkey_error = None;
    s.session.settings_ui.global_hotkey_error_candidate = None;
}

// BYPASS: bootstrap-only direct state mutation (see clear_global_hotkey_error).
fn set_global_hotkey_error(
    state: &Rc<RefCell<AppState>>,
    hotkey: String,
    error: GlobalHotkeyError,
) {
    let mut s = state.borrow_mut();
    s.session.settings_ui.global_hotkey_error = Some(error);
    s.session.settings_ui.global_hotkey_error_candidate = Some(hotkey);
}

// BYPASS: bootstrap-only direct state mutation (see clear_global_hotkey_error).
fn persist_hotkey_value(state: &Rc<RefCell<AppState>>, hotkey: String, reason: &str) {
    {
        let mut s = state.borrow_mut();
        s.session.settings.system.global_hotkey = hotkey;
    }

    let settings_saved = {
        let s = state.borrow();
        s.settings_writer.flush(s.session.settings.clone())
    };
    if !settings_saved {
        warn!(
            target: "settings",
            "failed to persist global hotkey after {}",
            reason
        );
    }
}

/// 监听二次实例的 SHOW 请求，桥接 std::sync::mpsc → 前台 executor
pub(crate) fn listen_for_secondary_instance(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_registration_keeps_valid_registered_hotkey() {
        assert_eq!(
            classify_startup_hotkey_registration("cmd-shift-s", Ok("cmd-shift-s".to_string())),
            StartupHotkeyRegistration::Registered {
                persisted: "cmd-shift-s".to_string(),
                canonicalized: false,
            }
        );
    }

    #[test]
    fn startup_registration_marks_legacy_display_format_for_canonicalization() {
        assert_eq!(
            classify_startup_hotkey_registration("Cmd+S", Ok("cmd-s".to_string())),
            StartupHotkeyRegistration::Registered {
                persisted: "cmd-s".to_string(),
                canonicalized: true,
            }
        );
    }

    #[test]
    fn startup_registration_recovers_only_for_invalid_configuration() {
        assert_eq!(
            classify_startup_hotkey_registration(
                "bad-hotkey",
                Err(GlobalHotkeyError::InvalidFormat)
            ),
            StartupHotkeyRegistration::RecoverWithDefault
        );
    }

    #[test]
    fn startup_registration_preserves_saved_hotkey_on_transient_failure() {
        let conflict = GlobalHotkeyError::Conflict("already in use".to_string());

        assert_eq!(
            classify_startup_hotkey_registration("cmd-s", Err(conflict.clone())),
            StartupHotkeyRegistration::KeepConfiguredError(conflict)
        );
    }

    #[test]
    fn startup_error_candidate_normalizes_legacy_display_format() {
        assert_eq!(
            normalize_hotkey_error_candidate("Cmd+S"),
            Some("cmd-s".to_string())
        );
    }
}
