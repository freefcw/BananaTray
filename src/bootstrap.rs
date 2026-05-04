//! 应用初始化 — 启动时调用一次的设置和注册函数

use crate::application::{AppAction, GlobalHotkeyError};
use crate::models::{AppSettings, SystemSettings};
use crate::refresh::{RefreshCoordinator, RefreshReason, RefreshRequest};
use crate::runtime::AppState;
use crate::tray::TrayController;
use gpui::{App, TrayIconClickEvent};
use log::{info, warn};
use rust_i18n::t;
use std::cell::RefCell;
use std::rc::Rc;

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

/// 初始化 i18n、UI 工具包、托盘图标（在 GPUI run 闭包内调用）
pub(crate) fn bootstrap_ui(cx: &mut App, settings: &AppSettings) {
    // i18n locale
    crate::i18n::apply_locale(&settings.display.language);
    crate::ui::settings_window::register_runtime_hooks();

    // adabraka-ui 工具包
    adabraka_ui::init(cx);
    adabraka_ui::theme::install_theme(cx, adabraka_ui::theme::Theme::light());
    cx.set_keep_alive_without_windows(true);

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
        crate::tray::apply_tray_icon(cx, icon_request);
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

/// 启动事件泵：从协调器接收 RefreshEvent，分派到 UI 线程更新 AppState
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
                    crate::runtime::dispatch_in_app(
                        &state,
                        AppAction::RefreshEventReceived(event),
                        cx,
                    );

                    // Linux: D-Bus 信号发射（reducer 已更新 AppState）
                    #[cfg(target_os = "linux")]
                    emit_dbus_signals(&state);
                });
            }
        })
        .detach();
}

/// 向 GNOME Shell Extension 发射 D-Bus 信号
#[cfg(target_os = "linux")]
fn emit_dbus_signals(state: &Rc<RefCell<AppState>>) {
    emit_current_dbus_snapshot(state);
}

/// 向 GNOME Shell Extension 发射当前状态快照。
#[cfg(target_os = "linux")]
pub(crate) fn emit_current_dbus_snapshot(state: &Rc<RefCell<AppState>>) {
    use crate::application::DBusQuotaSnapshot;

    let state_ref = state.borrow();
    if let Some(handle) = &state_ref.dbus_handle {
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
    let state = controller.borrow().state.clone();
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

fn clear_global_hotkey_error(state: &Rc<RefCell<AppState>>) {
    let mut s = state.borrow_mut();
    s.session.settings_ui.global_hotkey_error = None;
    s.session.settings_ui.global_hotkey_error_candidate = None;
}

fn set_global_hotkey_error(
    state: &Rc<RefCell<AppState>>,
    hotkey: String,
    error: GlobalHotkeyError,
) {
    let mut s = state.borrow_mut();
    s.session.settings_ui.global_hotkey_error = Some(error);
    s.session.settings_ui.global_hotkey_error_candidate = Some(hotkey);
}

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
