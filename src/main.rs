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

use gpui::{App, Application};
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

    let log_path = match platform::logging::init() {
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
        .with_assets(Assets::new())
        .run(move |cx: &mut App| {
            let settings = bootstrap::load_settings();

            // 1. UI + 托盘初始化
            bootstrap::bootstrap_ui(cx, &settings);

            // 2. 后台刷新系统
            let (refresh_tx, event_rx, manager) = bootstrap::bootstrap_refresh();

            bootstrap::sync_initial_auto_launch(&settings);

            // 3. 窗口控制器
            let controller = Rc::new(RefCell::new(tray::TrayController::new(
                refresh_tx,
                manager.clone(),
                settings,
                log_path.clone(),
            )));

            // 4. 事件泵
            bootstrap::start_event_pump(&controller.borrow().state, event_rx, cx);

            // 4.5 Linux: 启动 D-Bus 服务（供 GNOME Shell Extension 使用）
            #[cfg(target_os = "linux")]
            {
                let dbus_handle =
                    dbus::start_dbus_service(controller.borrow().state.clone(), cx.to_async());
                controller.borrow_mut().state.borrow_mut().dbus_handle = dbus_handle;
            }

            // 5. 初始刷新
            bootstrap::trigger_initial_refresh(&controller.borrow().state);

            // 6. 注册事件处理器
            bootstrap::register_tray_events(&controller, cx);
            bootstrap::register_global_hotkey(&controller, cx);
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
            eprintln!("usage: bananatray debug-codeium-family [antigravity|windsurf|all]");
            std::process::exit(2);
        }
    }
}
