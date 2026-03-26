use super::SettingsView;
use crate::app::persist_settings;
use crate::app::widgets::{
    render_card, render_card_separator, render_checkbox_row, render_section_label,
};
use crate::models::AppSettings;
use crate::theme::Theme;
use gpui::prelude::FluentBuilder;
use gpui::*;

/// Available refresh cadence options (in minutes)
const REFRESH_OPTIONS: &[u64] = &[1, 2, 3, 5, 10, 15, 30];

/// Format a cadence option for display
fn format_cadence(mins: u64) -> String {
    if mins == 1 {
        "1 minute".to_string()
    } else {
        format!("{} minutes", mins)
    }
}

impl SettingsView {
    /// Render General settings tab
    pub(super) fn render_general_tab(&self, settings: &AppSettings, theme: &Theme) -> Div {
        let state = self.state.clone();

        // ── SYSTEM section ───────────────────────────────────
        let login_state = state.clone();
        let login_checked = settings.start_at_login;

        // ── USAGE section ────────────────────────────────────
        let cost_state = state.clone();
        let cost_checked = settings.show_cost_summary;

        // ── AUTOMATION section ───────────────────────────────
        let cadence_mins = settings.refresh_interval_mins;
        let status_state = state.clone();
        let status_checked = settings.check_provider_status;
        let notif_state = state.clone();
        let notif_checked = settings.session_quota_notifications;

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
                            .child({
                                let dropdown_open = self.state.borrow().cadence_dropdown_open;
                                let toggle_state = state.clone();

                                let mut cadence_row = div()
                                    .flex_col()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .px(px(14.0))
                                            .py(px(10.0))
                                            .child(
                                                div()
                                                    .flex_col()
                                                    .gap(px(2.0))
                                                    .flex_1()
                                                    .child(
                                                        div()
                                                            .text_size(px(13.0))
                                                            .font_weight(FontWeight::MEDIUM)
                                                            .child("Refresh cadence"),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_size(px(12.5))
                                                            .line_height(relative(1.4))
                                                            .text_color(theme.text_secondary)
                                                            .child("How often BananaTray polls providers in the background."),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_shrink_0()
                                                    .items_center()
                                                    .gap(px(4.0))
                                                    .ml(px(12.0))
                                                    .px(px(10.0))
                                                    .py(px(5.0))
                                                    .rounded(px(6.0))
                                                    .bg(theme.bg_subtle)
                                                    .border_1()
                                                    .border_color(if dropdown_open { theme.element_selected } else { theme.border_strong })
                                                    .cursor_pointer()
                                                    .child(
                                                        div()
                                                            .text_size(px(12.0))
                                                            .font_weight(FontWeight::MEDIUM)
                                                            .text_color(theme.text_primary)
                                                            .child(format_cadence(cadence_mins)),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_size(px(10.0))
                                                            .text_color(theme.text_muted)
                                                            .ml(px(4.0))
                                                            .child(if dropdown_open { "▲" } else { "▼" }),
                                                    )
                                                    .on_mouse_down(MouseButton::Left, move |_, window, _| {
                                                        let mut s = toggle_state.borrow_mut();
                                                        s.cadence_dropdown_open = !s.cadence_dropdown_open;
                                                        drop(s);
                                                        window.refresh();
                                                    }),
                                            ),
                                    );

                                if dropdown_open {
                                    cadence_row = cadence_row.child(
                                        div()
                                            .flex_col()
                                            .mx(px(14.0))
                                            .mb(px(8.0))
                                            .rounded(px(8.0))
                                            .bg(theme.bg_subtle)
                                            .border_1()
                                            .border_color(theme.border_strong)
                                            .overflow_hidden()
                                            .children(REFRESH_OPTIONS.iter().enumerate().map(|(i, &mins)| {
                                                let is_active = cadence_mins == mins;
                                                let opt_state = state.clone();
                                                let mut row = div()
                                                    .flex()
                                                    .items_center()
                                                    .justify_between()
                                                    .px(px(12.0))
                                                    .py(px(7.0))
                                                    .cursor_pointer()
                                                    .bg(if is_active { theme.element_selected } else { transparent_black() })
                                                    .child(
                                                        div()
                                                            .text_size(px(12.5))
                                                            .font_weight(if is_active { FontWeight::SEMIBOLD } else { FontWeight::MEDIUM })
                                                            .text_color(if is_active { theme.element_active } else { theme.text_primary })
                                                            .child(format_cadence(mins)),
                                                    )
                                                    .when(is_active, |el| {
                                                        el.child(
                                                            div()
                                                                .text_size(px(11.0))
                                                                .font_weight(FontWeight::BOLD)
                                                                .text_color(theme.element_active)
                                                                .child("✓"),
                                                        )
                                                    })
                                                    .on_mouse_down(MouseButton::Left, move |_, window, _| {
                                                        let settings = {
                                                            let mut s = opt_state.borrow_mut();
                                                            s.settings.refresh_interval_mins = mins;
                                                            s.cadence_dropdown_open = false;
                                                            s.settings.clone()
                                                        };
                                                        persist_settings(&settings);
                                                        window.refresh();
                                                    });
                                                // separator between items (not before first)
                                                if i > 0 {
                                                    row = row.border_t_1().border_color(rgb(0xe0e0e4));
                                                }
                                                row
                                            })),
                                    );
                                }

                                cadence_row
                            })
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
