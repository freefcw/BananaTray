#![recursion_limit = "512"]

//! BananaTray - 系统托盘配额监控应用
//!
//! 这是一个 bin + lib 混合 crate；lib 持有唯一模块图，并在 `app` feature 下提供完整应用入口。

rust_i18n::i18n!("locales", fallback = "en");

pub mod application;
#[cfg(feature = "app")]
#[allow(dead_code)]
pub mod bootstrap;
mod builtin_provider_manifest;
#[cfg(all(target_os = "linux", feature = "app"))]
mod dbus;
pub mod i18n;
pub mod models;
pub mod platform;
pub mod providers;
pub mod refresh;
pub mod settings_store;
#[cfg(feature = "app")]
pub mod theme;
pub mod utils;

// GPUI 视图层和运行时模块，测试时不编译
#[cfg(feature = "app")]
pub mod runtime;
#[cfg(feature = "app")]
pub mod tray;
#[cfg(feature = "app")]
pub mod ui;

/// 启动完整的 BananaTray 托盘应用。
///
/// binary target 只负责调用此入口，确保模块图和初始化逻辑只有 lib crate 一份。
#[cfg(feature = "app")]
pub fn run_app() {
    use gpui::{App, AppProfile, Application};
    use log::info;
    use platform::assets::Assets;
    use rust_i18n::t;
    use std::cell::RefCell;
    use std::rc::Rc;

    if try_run_codeium_family_debug_cli() {
        return;
    }

    // 启动早期加载设置：日志轮转/清理阈值依赖 logging 子配置，
    // 必须在 platform::logging::init 之前读取；同一份 settings 移入 GPUI run 闭包复用。
    // 加载失败（文件损坏等）时先备份原文件再回退默认值，
    // 避免后续 persist 把默认值写回后原始内容不可恢复。
    let mut corrupt_backup: Option<std::path::PathBuf> = None;
    let settings = crate::settings_store::load().unwrap_or_else(|err| {
        eprintln!("failed to load settings: {err:#}");
        corrupt_backup = crate::settings_store::backup_corrupt_file();
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

    // 只在 GPUI 事件循环启动前允许同步平台探测；进入前台后调用点只读取缓存，
    // 过期刷新由后台线程完成，避免系统命令阻塞窗口构造或托盘更新。
    platform::system::prewarm_system_dark_mode_detection();
    #[cfg(target_os = "linux")]
    platform::gnome_detect::prewarm_gnome_extension_detection();

    Application::new()
        .with_resource_profile(AppProfile::Minimal)
        .with_assets(Assets::new())
        .run(move |cx: &mut App| {
            // 1. UI + 托盘初始化
            bootstrap::bootstrap_ui(cx, &settings);

            // 设置文件损坏恢复提示：备份成功时告知用户备份位置与默认值回退
            if let Some(backup_path) = &corrupt_backup {
                let title = t!("settings.corrupt_backup.title").to_string();
                let body = t!(
                    "settings.corrupt_backup.body",
                    path = backup_path.display().to_string()
                )
                .to_string();
                platform::notification::send_plain_notification(&title, &body);
            }

            // 2. 后台刷新系统
            let (refresh_tx, event_rx, manager) = bootstrap::bootstrap_refresh();
            let (script_test_tx, script_test_rx) = bootstrap::script_test_channel();
            let (custom_provider_tx, custom_provider_rx) = bootstrap::custom_provider_channel();

            bootstrap::sync_initial_auto_launch(&settings);

            // 3. 组合共享运行时状态与窗口控制器
            let state = Rc::new(RefCell::new(runtime::AppState::new(
                refresh_tx,
                custom_provider_tx,
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
            bootstrap::start_custom_provider_pump(&state, custom_provider_rx, cx);

            // 7. 初始刷新
            bootstrap::trigger_initial_refresh(&state);

            // 8. 注册事件处理器
            bootstrap::register_tray_events(&controller, cx);
            bootstrap::register_global_hotkey(&state, &controller, cx);
            bootstrap::listen_for_secondary_instance(&controller, show_rx, cx);

            info!(target: "app", "BananaTray is running - look for the tray icon");
        });
}

#[cfg(feature = "app")]
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
