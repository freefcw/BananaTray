#![recursion_limit = "512"]

rust_i18n::i18n!("locales", fallback = "en");

mod application;
mod bootstrap;
mod builtin_provider_manifest;
#[cfg(target_os = "linux")]
mod dbus;
mod i18n;
pub mod models;
mod platform;
mod providers;
mod refresh;
mod runtime;
mod settings_store;
mod theme;
mod tray;
mod ui;
mod utils;

use gpui::{App, AppProfile, Application};
use log::info;
use platform::assets::Assets;
use std::cell::RefCell;
use std::rc::Rc;

// ============================================================================
// Entry Point
// ============================================================================

fn main() {
    if try_run_codeium_family_debug_cli() {
        return;
    }

    // 启动早期加载设置：日志轮转/清理阈值依赖 logging 子配置，
    // 必须在 platform::logging::init 之前读取；同一份 settings 移入 GPUI run 闭包复用。
    let settings = crate::settings_store::load().unwrap_or_else(|err| {
        eprintln!("failed to load settings: {err:#}");
        Default::default()
    });
    let log_path = match platform::logging::init(&settings.logging) {
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
    let show_rx = match platform::single_instance::ensure_single_instance() {
        platform::single_instance::InstanceRole::Primary(rx) => rx,
        platform::single_instance::InstanceRole::Secondary => {
            info!(target: "app", "another instance is already running, exiting");
            std::process::exit(0);
        }
    };

    Application::new()
        .with_resource_profile(AppProfile::Minimal)
        .with_assets(Assets::new())
        .run(move |cx: &mut App| {
            // 1. UI + 托盘初始化
            bootstrap::bootstrap_ui(cx, &settings);

            // 2. 后台刷新系统
            let (refresh_tx, event_rx, manager) = bootstrap::bootstrap_refresh();
            let (script_test_tx, script_test_rx) = bootstrap::script_test_channel();

            bootstrap::sync_initial_auto_launch(&settings);

            // 3. 组合共享运行时状态与窗口控制器
            let state = Rc::new(RefCell::new(runtime::AppState::new(
                refresh_tx,
                script_test_tx,
                manager.clone(),
                settings,
                log_path.clone(),
            )));
            let controller = Rc::new(RefCell::new(tray::TrayController::new(state.clone())));

            // 4. 注册应用退出钩子（等待设置最终落盘）
            bootstrap::register_app_shutdown(&state, cx);

            // 5. Linux: 启动 D-Bus 服务（供 GNOME Shell Extension 使用）
            #[cfg(target_os = "linux")]
            let dbus_handle = {
                let handle = dbus::start_dbus_service(state.clone(), cx.to_async());
                bootstrap::emit_current_dbus_snapshot(&state, handle.as_ref());
                handle
            };

            // 6. 事件泵
            #[cfg(target_os = "linux")]
            bootstrap::start_event_pump(&state, event_rx, dbus_handle, cx);
            #[cfg(not(target_os = "linux"))]
            bootstrap::start_event_pump(&state, event_rx, cx);
            bootstrap::start_script_test_pump(&state, script_test_rx, cx);

            // 7. 初始刷新
            bootstrap::trigger_initial_refresh(&state);

            // 8. 注册事件处理器
            bootstrap::register_tray_events(&controller, cx);
            bootstrap::register_global_hotkey(&state, &controller, cx);
            bootstrap::listen_for_secondary_instance(&controller, show_rx, cx);

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
            eprintln!("usage: bananatray debug-codeium-family [antigravity|devin|windsurf|all]");
            std::process::exit(2);
        }
    }
}
