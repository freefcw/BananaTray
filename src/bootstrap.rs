//! 应用初始化 — 启动时调用一次的设置和注册函数

use crate::application::AppAction;
use crate::models::AppSettings;
use crate::refresh::{RefreshCoordinator, RefreshReason, RefreshRequest};
use crate::runtime::AppState;
use crate::tray::TrayController;
use gpui::{App, Keystroke, TrayIconEvent};
use log::{info, warn};
use rust_i18n::t;
use std::cell::RefCell;
use std::rc::Rc;

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

    // 系统托盘
    let icon_request = match settings.display.tray_icon_style {
        crate::models::TrayIconStyle::Dynamic => {
            // 启动时数据尚未加载，默认 Green（= Monochrome），首次刷新后会自动更新
            crate::application::TrayIconRequest::DynamicStatus(crate::models::StatusLevel::Green)
        }
        style => crate::application::TrayIconRequest::Static(style),
    };
    crate::tray::apply_tray_icon(cx, icon_request);
    cx.set_tray_tooltip(&t!("tray.tooltip"));
    cx.set_tray_panel_mode(true);

    // 通知授权（仅在 App Bundle 模式下请求）
    crate::platform::notification::request_notification_authorization();
}

/// 创建 ProviderManager + RefreshCoordinator，启动后台刷新线程。
/// 返回 (refresh_tx, event_rx, manager) 供后续步骤使用。
pub(crate) fn bootstrap_refresh() -> (
    smol::channel::Sender<RefreshRequest>,
    smol::channel::Receiver<crate::refresh::RefreshEvent>,
    std::sync::Arc<crate::providers::ProviderManager>,
) {
    if let Err(err) = crate::platform::paths::migrate_legacy_custom_providers_dir() {
        warn!(target: "providers::custom", "failed to migrate legacy custom providers dir: {err}");
    }

    let (event_tx, event_rx) = smol::channel::bounded::<crate::refresh::RefreshEvent>(64);
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
                });
            }
        })
        .detach();
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

/// 注册托盘图标事件（左键/右键）
pub(crate) fn register_tray_events(controller: &Rc<RefCell<TrayController>>, cx: &mut App) {
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
pub(crate) fn register_global_hotkey(controller: &Rc<RefCell<TrayController>>, cx: &mut App) {
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
