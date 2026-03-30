use super::SettingsView;
use crate::app::persist_settings;
use crate::app::widgets::{
    render_cadence_dropdown, render_card, render_card_separator, render_checkbox_row,
    render_section_label,
};
use crate::models::AppSettings;
use crate::theme::Theme;
use gpui::*;

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
        let cadence_mins = settings.refresh_interval_mins;
        let status_state = state.clone();
        let status_checked = settings.check_provider_status;
        let notif_state = state.clone();
        let notif_checked = settings.session_quota_notifications;
        let sound_state = state.clone();
        let sound_checked = settings.notification_sound;

        div()
            .flex_col()
            .flex_1()
            .px(px(16.0))
            .pt(px(16.0))
            .pb(px(20.0))
            // ═══════ SYSTEM ═══════
            .child(
                div()
                    .flex_col()
                    .child(render_section_label("SYSTEM", theme))
                    .child(
                        render_card()
                            .child(
                                render_checkbox_row(
                                    "Start at Login",
                                    "Automatically opens BananaTray when you start your Mac.",
                                    login_checked,
                                    theme,
                                )
                                .on_mouse_down(MouseButton::Left, move |_, window, _| {
                                    let settings = {
                                        let mut s = login_state.borrow_mut();
                                        s.settings.start_at_login = !s.settings.start_at_login;
                                        s.settings.clone()
                                    };
                                    persist_settings(&settings);
                                    window.refresh();
                                }),
                            ),
                    ),
            )
            // ═══════ TOOLBAR ═══════
            .child(
                div()
                    .flex_col()
                    .mt(px(12.0))
                    .child(render_section_label("TOOLBAR", theme))
                    .child(
                        render_card()
                            .child(
                                render_checkbox_row(
                                    "Show Dashboard button",
                                    "Display the Dashboard button in the popup toolbar.",
                                    dash_checked,
                                    theme,
                                )
                                .on_mouse_down(MouseButton::Left, move |_, window, _| {
                                    let settings = {
                                        let mut s = dash_state.borrow_mut();
                                        s.settings.show_toolbar_dashboard =
                                            !s.settings.show_toolbar_dashboard;
                                        s.settings.clone()
                                    };
                                    persist_settings(&settings);
                                    window.refresh();
                                }),
                            )
                            .child(render_card_separator())
                            .child(
                                render_checkbox_row(
                                    "Show Refresh button",
                                    "Display the Refresh button in the popup toolbar.",
                                    refresh_checked,
                                    theme,
                                )
                                .on_mouse_down(MouseButton::Left, move |_, window, _| {
                                    let settings = {
                                        let mut s = refresh_btn_state.borrow_mut();
                                        s.settings.show_toolbar_refresh =
                                            !s.settings.show_toolbar_refresh;
                                        s.settings.clone()
                                    };
                                    persist_settings(&settings);
                                    window.refresh();
                                }),
                            ),
                    ),
            )
            // ═══════ USAGE ═══════
            .child(
                div()
                    .flex_col()
                    .mt(px(12.0))
                    .child(render_section_label("USAGE", theme))
                    .child(
                        render_card()
                            .child(
                                render_checkbox_row(
                                    "Show cost summary",
                                    "Reads local usage logs. Shows today + last 30 days cost in the menu.",
                                    cost_checked,
                                    theme,
                                )
                                .on_mouse_down(MouseButton::Left, move |_, window, _| {
                                    let settings = {
                                        let mut s = cost_state.borrow_mut();
                                        s.settings.show_cost_summary = !s.settings.show_cost_summary;
                                        s.settings.clone()
                                    };
                                    persist_settings(&settings);
                                    window.refresh();
                                }),
                            ),
                    ),
            )
            // ═══════ AUTOMATION ═══════
            .child(
                div()
                    .flex_col()
                    .mt(px(12.0))
                    .child(render_section_label("AUTOMATION", theme))
                    .child(
                        render_card()
                            // Refresh cadence (dropdown)
                            .child(render_cadence_dropdown(&state, cadence_mins, theme))
                            .child(render_card_separator())
                            // Check provider status
                            .child(
                                render_checkbox_row(
                                    "Check provider status",
                                    "Polls provider status pages, surfacing incidents in the icon and menu.",
                                    status_checked,
                                    theme,
                                )
                                .on_mouse_down(MouseButton::Left, move |_, window, _| {
                                    let settings = {
                                        let mut s = status_state.borrow_mut();
                                        s.settings.check_provider_status = !s.settings.check_provider_status;
                                        s.settings.clone()
                                    };
                                    persist_settings(&settings);
                                    window.refresh();
                                }),
                            )
                            .child(render_card_separator())
                            // Session quota notifications
                            .child(
                                render_checkbox_row(
                                    "Session quota notifications",
                                    "Notifies when the 5-hour session quota hits 0% and when it becomes available again.",
                                    notif_checked,
                                    theme,
                                )
                                .on_mouse_down(MouseButton::Left, move |_, window, _| {
                                    let settings = {
                                        let mut s = notif_state.borrow_mut();
                                        s.settings.session_quota_notifications = !s.settings.session_quota_notifications;
                                        s.settings.clone()
                                    };
                                    persist_settings(&settings);
                                    window.refresh();
                                }),
                            )
                            .child(render_card_separator())
                            // Notification sound
                            .child(
                                render_checkbox_row(
                                    "Notification sound",
                                    "Play a sound when sending quota notifications.",
                                    sound_checked,
                                    theme,
                                )
                                .on_mouse_down(MouseButton::Left, move |_, window, _| {
                                    let settings = {
                                        let mut s = sound_state.borrow_mut();
                                        s.settings.notification_sound = !s.settings.notification_sound;
                                        s.settings.clone()
                                    };
                                    persist_settings(&settings);
                                    window.refresh();
                                }),
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
                            .child("Quit BananaTray")
                            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                cx.quit();
                            }),
                    ),
            )
    }
}
