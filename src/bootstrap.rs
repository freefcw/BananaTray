//! 应用初始化 shell composition root。
//!
//! `bootstrap` 组合具体 GPUI window、tray、D-Bus handle 和 App/Window 级
//! dispatch facade；`runtime` 只接收能力抽象。子模块按 shell 生命周期边界拆分，
//! 这里保持稳定入口与对外 re-export。

mod capabilities;
mod event_sources;
mod settings_window;
mod ui_bootstrap;
mod workers;

/// 在 GPUI foreground executor 上泵送 channel：每收到一个事件，回到主线程执行一次回调。
///
/// 收敛 workers / event_sources / dbus 桥接中重复出现的 pump 脚手架；drain、dispatch、
/// 信号发射等差异逻辑由各调用方的 `on_event` 闭包表达。发送端全部关闭后泵自然退出。
pub(crate) fn spawn_foreground_pump<T, F>(
    async_cx: gpui::AsyncApp,
    rx: smol::channel::Receiver<T>,
    mut on_event: F,
) where
    T: 'static,
    F: FnMut(T, &mut gpui::App) + 'static,
{
    let pump_cx = async_cx.clone();
    async_cx
        .foreground_executor()
        .spawn(async move {
            while let Ok(event) = rx.recv().await {
                let _ = pump_cx.update(|cx| on_event(event, cx));
            }
        })
        .detach();
}

// These re-exports are the stable shell entry points for `main.rs` and GPUI
// callbacks. The lib test target compiles this module without the bin startup
// path, so some startup-only entries are intentionally unused there.
#[allow(unused_imports)]
pub(crate) use capabilities::{dispatch_in_app, dispatch_in_window};
#[allow(unused_imports)]
pub(crate) use event_sources::hotkey::register_global_hotkey;
#[allow(unused_imports)]
pub(crate) use event_sources::secondary_instance::listen_for_secondary_instance;
#[allow(unused_imports)]
pub(crate) use event_sources::shutdown::register_app_shutdown;
#[allow(unused_imports)]
pub(crate) use event_sources::tray::register_tray_events;
pub(crate) use settings_window::{
    clear_popup_view, register_settings_window_lifecycle, schedule_open_settings_window,
};
#[allow(unused_imports)]
pub(crate) use ui_bootstrap::{bootstrap_ui, sync_initial_auto_launch};
#[allow(unused_imports)]
pub(crate) use workers::clipboard::{clipboard_channel, start_clipboard_worker};
#[allow(unused_imports)]
pub(crate) use workers::custom_provider::{custom_provider_channel, start_custom_provider_pump};
#[allow(unused_imports)]
pub(crate) use workers::history::start_history_pump;
#[allow(unused_imports)]
#[cfg(target_os = "linux")]
pub(crate) use workers::linux_dbus::emit_current_dbus_snapshot;
#[allow(unused_imports)]
pub(crate) use workers::refresh::{bootstrap_refresh, start_event_pump, trigger_initial_refresh};
#[allow(unused_imports)]
pub(crate) use workers::script_test::{script_test_channel, start_script_test_pump};
pub(crate) use workers::token_input::start_token_input_pump;
