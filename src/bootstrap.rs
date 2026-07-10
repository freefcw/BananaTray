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
    clear_popup_view, register_build_settings_view, register_clear_popup_view,
    register_notify_view, schedule_open_settings_window,
};
#[allow(unused_imports)]
pub(crate) use ui_bootstrap::{bootstrap_ui, load_settings, sync_initial_auto_launch};
#[allow(unused_imports)]
#[cfg(target_os = "linux")]
pub(crate) use workers::linux_dbus::emit_current_dbus_snapshot;
#[allow(unused_imports)]
pub(crate) use workers::refresh::{bootstrap_refresh, start_event_pump, trigger_initial_refresh};
#[allow(unused_imports)]
pub(crate) use workers::script_test::{script_test_channel, start_script_test_pump};
