use super::SettingsView;
use crate::app::persist_settings;
use crate::app::widgets::{
    render_cadence_dropdown, render_card, render_card_separator, render_section_label,
    render_switch_row,
};
use crate::auto_launch;
use crate::models::AppSettings;
use crate::theme::Theme;
use gpui::*;
use rust_i18n::t;

impl SettingsView {
    /// Render General settings tab
    pub(super) fn render_general_tab(&self, settings: &AppSettings, theme: &Theme) -> Div {
        let state = self.state.clone();

        // ── SYSTEM section ───────────────────────────────────
        let login_state = state.clone();
        let login_checked = settings.start_at_login;

        // ── AUTOMATION section ───────────────────────────────
        let cadence_mins = if settings.refresh_interval_mins == 0 {
            None
        } else {
            Some(settings.refresh_interval_mins)
        };
        let status_state = state.clone();
        let status_checked = settings.check_provider_status;
        let notif_state = state.clone();
        let notif_checked = settings.session_quota_notifications;
        let sound_state = state.clone();
        let sound_checked = settings.notification_sound;

        div()
            .flex_col()
            .px(px(16.0))
            .pt(px(16.0))
            .pb(px(20.0))
            // ═══════ SYSTEM ═══════
            .child(
                div()
                    .flex_col()
                    .child(render_section_label(&t!("settings.section.system"), theme))
                    .child(
                        render_card().child(
                            render_switch_row(
                                &t!("settings.start_at_login"),
                                &t!("settings.start_at_login.desc"),
                                login_checked,
                                theme,
                                move |_, window, _| {
                                    let desired = {
                                        let mut s = login_state.borrow_mut();
                                        s.settings.start_at_login = !s.settings.start_at_login;
                                        persist_settings(&s.settings);
                                        s.settings.start_at_login
                                    };
                                    window.refresh();

                                    // 后台线程处理系统调用，避免阻塞UI
                                    std::thread::spawn(move || {
                                        auto_launch::sync(desired);

                                        let (title, body) = if desired {
                                            (t!("notification.auto_launch.enabled.title"),
                                             t!("notification.auto_launch.enabled.body"))
                                        } else {
                                            (t!("notification.auto_launch.disabled.title"),
                                             t!("notification.auto_launch.disabled.body"))
                                        };

                                        if let Err(e) = notify_rust::Notification::new()
                                            .appname("BananaTray")
                                            .summary(title.as_ref())
                                            .body(body.as_ref())
                                            .show()
                                        {
                                            log::warn!(target: "settings", "failed to show auto-launch notification: {}", e);
                                        }
                                    });
                                },
                            ),
                        ),
                    ),
            )
            // ═══════ AUTOMATION ═══════
            .child(
                div()
                    .flex_col()
                    .mt(px(12.0))
                    .child(render_section_label(
                        &t!("settings.section.automation"),
                        theme,
                    ))
                    .child(
                        render_card()
                            // Refresh cadence (dropdown)
                            .child(render_cadence_dropdown(&state, cadence_mins, theme))
                            .child(render_card_separator())
                            // Check provider status
                            .child(
                                render_switch_row(
                                    &t!("settings.check_provider_status"),
                                    &t!("settings.check_provider_status.desc"),
                                    status_checked,
                                    theme,
                                    move |_, window, _| {
                                        let settings = {
                                            let mut s = status_state.borrow_mut();
                                            s.settings.check_provider_status =
                                                !s.settings.check_provider_status;
                                            s.settings.clone()
                                        };
                                        persist_settings(&settings);
                                        window.refresh();
                                    },
                                ),
                            )
                            .child(render_card_separator())
                            // Session quota notifications
                            .child(
                                render_switch_row(
                                    &t!("settings.session_quota_notifications"),
                                    &t!("settings.session_quota_notifications.desc"),
                                    notif_checked,
                                    theme,
                                    move |_, window, _| {
                                        let settings = {
                                            let mut s = notif_state.borrow_mut();
                                            s.settings.session_quota_notifications =
                                                !s.settings.session_quota_notifications;
                                            s.settings.clone()
                                        };
                                        persist_settings(&settings);
                                        window.refresh();
                                    },
                                ),
                            )
                            .child(render_card_separator())
                            // Notification sound
                            .child(
                                render_switch_row(
                                    &t!("settings.notification_sound"),
                                    &t!("settings.notification_sound.desc"),
                                    sound_checked,
                                    theme,
                                    move |_, window, _| {
                                        let settings = {
                                            let mut s = sound_state.borrow_mut();
                                            s.settings.notification_sound =
                                                !s.settings.notification_sound;
                                            s.settings.clone()
                                        };
                                        persist_settings(&settings);
                                        window.refresh();
                                    },
                                ),
                            ),
                    ),
            )
            // ═══════ Quit ═══════
            .child(
                div()
                    .flex()
                    .justify_center()
                    .mt(px(12.0))
                    .pb(px(4.0))
                    .child(
                        div()
                            .px(px(22.0))
                            .py(px(8.0))
                            .rounded_full()
                            .bg(theme.status_error)
                            .text_size(px(13.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.element_active)
                            .cursor_pointer()
                            .child(t!("settings.quit").to_string())
                            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                cx.quit();
                            }),
                    ),
            )
    }
}
