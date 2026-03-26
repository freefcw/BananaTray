use super::SettingsView;
use crate::app::persist_settings;
use crate::models::AppSettings;
use crate::theme::Theme;
use gpui::*;

/// Available refresh cadence options (in minutes)
const REFRESH_OPTIONS: &[u64] = &[1, 2, 3, 5, 10, 15, 30];

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
                    .child(Self::render_section_label("SYSTEM", theme))
                    .child(
                        Self::render_card()
                            .child(
                                div()
                                    .flex()
                                    .items_start()
                                    .gap(px(10.0))
                                    .px(px(14.0))
                                    .py(px(10.0))
                                    .cursor_pointer()
                                    .child(self.render_settings_checkbox(login_checked, theme))
                                    .child(
                                        div()
                                            .flex_col()
                                            .gap(px(2.0))
                                            .child(
                                                div()
                                                    .text_size(px(13.0))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .child("Start at Login"),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(12.5))
                                                    .line_height(relative(1.4))
                                                    .text_color(theme.text_secondary)
                                                    .child("Automatically opens BananaTray when you start your Mac."),
                                            ),
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
                    .child(Self::render_section_label("USAGE", theme))
                    .child(
                        Self::render_card()
                            .child(
                                div()
                                    .flex()
                                    .items_start()
                                    .gap(px(10.0))
                                    .px(px(14.0))
                                    .py(px(10.0))
                                    .cursor_pointer()
                                    .child(self.render_settings_checkbox(cost_checked, theme))
                                    .child(
                                        div()
                                            .flex_col()
                                            .gap(px(2.0))
                                            .child(
                                                div()
                                                    .text_size(px(13.0))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .child("Show cost summary"),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(12.5))
                                                    .line_height(relative(1.4))
                                                    .text_color(theme.text_secondary)
                                                    .child("Reads local usage logs. Shows today + last 30 days cost in the menu."),
                                            ),
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
                    .child(Self::render_section_label("AUTOMATION", theme))
                    .child(
                        Self::render_card()
                            // Refresh cadence
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
                                            .children(REFRESH_OPTIONS.iter().map(|&mins| {
                                                let is_active = cadence_mins == mins;
                                                let opt_state = state.clone();
                                                div()
                                                    .min_w(px(32.0))
                                                    .px(px(6.0))
                                                    .py(px(4.0))
                                                    .rounded(px(6.0))
                                                    .bg(if is_active { theme.element_selected } else { theme.bg_subtle })
                                                    .border_1()
                                                    .border_color(if is_active { theme.element_selected } else { theme.border_strong })
                                                    .text_size(px(11.0))
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .text_color(if is_active { theme.element_active } else { theme.text_primary })
                                                    .cursor_pointer()
                                                    .flex()
                                                    .justify_center()
                                                    .child(format!("{}m", mins))
                                                    .on_mouse_down(MouseButton::Left, move |_, window, _| {
                                                        let settings = {
                                                            let mut s = opt_state.borrow_mut();
                                                            s.settings.refresh_interval_mins = mins;
                                                            s.settings.clone()
                                                        };
                                                        persist_settings(&settings);
                                                        window.refresh();
                                                    })
                                            })),
                                    ),
                            )
                            .child(Self::render_card_separator())
                            // Check provider status
                            .child(
                                div()
                                    .flex()
                                    .items_start()
                                    .gap(px(10.0))
                                    .px(px(14.0))
                                    .py(px(10.0))
                                    .cursor_pointer()
                                    .child(self.render_settings_checkbox(status_checked, theme))
                                    .child(
                                        div()
                                            .flex_col()
                                            .gap(px(2.0))
                                            .child(
                                                div()
                                                    .text_size(px(13.0))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .child("Check provider status"),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(12.5))
                                                    .line_height(relative(1.4))
                                                    .text_color(theme.text_secondary)
                                                    .child("Polls provider status pages, surfacing incidents in the icon and menu."),
                                            ),
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
                            .child(Self::render_card_separator())
                            // Session quota notifications
                            .child(
                                div()
                                    .flex()
                                    .items_start()
                                    .gap(px(10.0))
                                    .px(px(14.0))
                                    .py(px(10.0))
                                    .cursor_pointer()
                                    .child(self.render_settings_checkbox(notif_checked, theme))
                                    .child(
                                        div()
                                            .flex_col()
                                            .gap(px(2.0))
                                            .child(
                                                div()
                                                    .text_size(px(13.0))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .child("Session quota notifications"),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(12.5))
                                                    .line_height(relative(1.4))
                                                    .text_color(theme.text_secondary)
                                                    .child("Notifies when the 5-hour session quota hits 0% and when it becomes available again."),
                                            ),
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
