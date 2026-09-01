use crate::models::AppSettings;
use gpui::{App, QuitMode};
use log::info;
use rust_i18n::t;

pub(crate) fn sync_initial_auto_launch(settings: &AppSettings) {
    crate::platform::auto_launch::schedule_sync(settings.system.start_at_login);
}

/// 初始化 i18n、UI 工具包、托盘图标（在 GPUI run 闭包内调用）
pub(crate) fn bootstrap_ui(cx: &mut App, settings: &AppSettings) {
    crate::i18n::apply_locale(&settings.display.language);
    crate::ui::register_shell_hooks();

    adabraka_ui::init(cx);
    adabraka_ui::theme::install_theme(cx, adabraka_ui::theme::Theme::light());
    // 托盘应用在所有窗口关闭后仍需常驻，只允许显式退出。
    cx.set_quit_mode(QuitMode::Explicit);
    crate::runtime::register_idle_gpu_cache_trim(cx);

    if crate::tray::should_use_gpui_tray() {
        let icon_request = match settings.display.tray_icon_style {
            crate::models::TrayIconStyle::Dynamic => {
                // 启动时数据尚未加载，默认 Green（= Monochrome），首次刷新后会自动更新。
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

    crate::platform::notification::request_notification_authorization();
}
