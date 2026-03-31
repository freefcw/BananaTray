use super::SettingsView;
use crate::app::persist_settings;
use crate::app::widgets::{
    render_cadence_dropdown, render_card, render_card_separator, render_checkbox_row,
    render_section_label,
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

        // ── TOOLBAR section ─────────────────────────────────
        let dash_state = state.clone();
        let dash_checked = settings.show_toolbar_dashboard;
        let refresh_btn_state = state.clone();
        let refresh_checked = settings.show_toolbar_refresh;

        // ── USAGE section ────────────────────────────────────
        let cost_state = state.clone();
        let cost_checked = settings.show_cost_summary;

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

        // ── LANGUAGE section ─────────────────────────────────
        let current_language = settings.language.clone();

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
                            render_checkbox_row(
                                &t!("settings.start_at_login"),
                                &t!("settings.start_at_login.desc"),
                                login_checked,
                                theme,
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                move |_, window, _| {
                                    let settings = {
                                        let mut s = login_state.borrow_mut();
                                        s.settings.start_at_login = !s.settings.start_at_login;
                                        s.settings.clone()
                                    };
                                    auto_launch::sync(settings.start_at_login);
                                    persist_settings(&settings);
                                    window.refresh();
                                },
                            ),
                        ),
                    ),
            )
            // ═══════ LANGUAGE ═══════
            .child(
                div()
                    .flex_col()
                    .mt(px(12.0))
                    .child(render_section_label(
                        &t!("settings.section.language"),
                        theme,
                    ))
                    .child(self.render_language_selector(&current_language, theme)),
            )
            // ═══════ TOOLBAR ═══════
            .child(
                div()
                    .flex_col()
                    .mt(px(12.0))
                    .child(render_section_label(&t!("settings.section.toolbar"), theme))
                    .child(
                        render_card()
                            .child(
                                render_checkbox_row(
                                    &t!("settings.show_dashboard"),
                                    &t!("settings.show_dashboard.desc"),
                                    dash_checked,
                                    theme,
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    move |_, window, _| {
                                        let settings = {
                                            let mut s = dash_state.borrow_mut();
                                            s.settings.show_toolbar_dashboard =
                                                !s.settings.show_toolbar_dashboard;
                                            s.settings.clone()
                                        };
                                        persist_settings(&settings);
                                        window.refresh();
                                    },
                                ),
                            )
                            .child(render_card_separator())
                            .child(
                                render_checkbox_row(
                                    &t!("settings.show_refresh"),
                                    &t!("settings.show_refresh.desc"),
                                    refresh_checked,
                                    theme,
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    move |_, window, _| {
                                        let settings = {
                                            let mut s = refresh_btn_state.borrow_mut();
                                            s.settings.show_toolbar_refresh =
                                                !s.settings.show_toolbar_refresh;
                                            s.settings.clone()
                                        };
                                        persist_settings(&settings);
                                        window.refresh();
                                    },
                                ),
                            ),
                    ),
            )
            // ═══════ USAGE ═══════
            .child(
                div()
                    .flex_col()
                    .mt(px(12.0))
                    .child(render_section_label(&t!("settings.section.usage"), theme))
                    .child(
                        render_card().child(
                            render_checkbox_row(
                                &t!("settings.show_cost_summary"),
                                &t!("settings.show_cost_summary.desc"),
                                cost_checked,
                                theme,
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                move |_, window, _| {
                                    let settings = {
                                        let mut s = cost_state.borrow_mut();
                                        s.settings.show_cost_summary =
                                            !s.settings.show_cost_summary;
                                        s.settings.clone()
                                    };
                                    persist_settings(&settings);
                                    window.refresh();
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
                                render_checkbox_row(
                                    &t!("settings.check_provider_status"),
                                    &t!("settings.check_provider_status.desc"),
                                    status_checked,
                                    theme,
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
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
                                render_checkbox_row(
                                    &t!("settings.session_quota_notifications"),
                                    &t!("settings.session_quota_notifications.desc"),
                                    notif_checked,
                                    theme,
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
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
                                render_checkbox_row(
                                    &t!("settings.notification_sound"),
                                    &t!("settings.notification_sound.desc"),
                                    sound_checked,
                                    theme,
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
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

    /// Render language selector card with radio-button style options
    fn render_language_selector(&self, current: &str, theme: &Theme) -> Div {
        use crate::i18n::SUPPORTED_LANGUAGES;

        let mut card = render_card()
            .flex_col()
            .px(px(14.0))
            .py(px(10.0))
            .gap(px(2.0))
            .child(
                div()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::MEDIUM)
                            .child(t!("settings.language").to_string()),
                    )
                    .child(
                        div()
                            .text_size(px(12.5))
                            .line_height(relative(1.4))
                            .text_color(theme.text_secondary)
                            .child(t!("settings.language.desc").to_string()),
                    ),
            );

        let mut options = div().flex().flex_wrap().gap(px(6.0)).mt(px(8.0));
        for &(code, name_key) in SUPPORTED_LANGUAGES {
            let is_active = current == code;
            let state = self.state.clone();
            let code_owned = code.to_string();

            options = options.child(
                div()
                    .px(px(12.0))
                    .py(px(6.0))
                    .rounded_full()
                    .bg(if is_active {
                        theme.element_selected
                    } else {
                        theme.bg_subtle
                    })
                    .border_1()
                    .border_color(if is_active {
                        theme.element_selected
                    } else {
                        theme.border_subtle
                    })
                    .text_size(px(12.0))
                    .font_weight(if is_active {
                        FontWeight::SEMIBOLD
                    } else {
                        FontWeight::MEDIUM
                    })
                    .text_color(if is_active {
                        theme.element_active
                    } else {
                        theme.text_primary
                    })
                    .cursor_pointer()
                    .child(t!(name_key).to_string())
                    .on_mouse_down(MouseButton::Left, move |_, window, _| {
                        let settings = {
                            let mut s = state.borrow_mut();
                            s.settings.language = code_owned.clone();
                            crate::i18n::apply_locale(&s.settings.language);
                            s.settings.clone()
                        };
                        persist_settings(&settings);
                        window.refresh();
                    }),
            );
        }

        card = card.child(options);
        card
    }
}
