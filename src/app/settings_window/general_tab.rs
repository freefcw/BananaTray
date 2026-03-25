use super::SettingsView;
use crate::models::AppSettings;
use crate::theme::Theme;
use gpui::*;

impl SettingsView {
    /// Render General settings tab
    pub(super) fn render_general_tab(&self, settings: &AppSettings, theme: &Theme) -> Div {
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
                                    .child(self.render_settings_checkbox(false, theme))
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
                                    ),
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
                                    .child(self.render_settings_checkbox(true, theme))
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
                                            )
                                            .child(
                                                div()
                                                    .flex_col()
                                                    .gap(px(1.0))
                                                    .mt(px(4.0))
                                                    .text_size(px(11.5))
                                                    .text_color(theme.text_muted)
                                                    .child(div().child("Auto-refresh: hourly · Timeout: 10m"))
                                                    .child(div().child("Claude: no data yet"))
                                                    .child(div().child("Codex: no data yet")),
                                            ),
                                    ),
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
                                            .gap(px(3.0))
                                            .ml(px(12.0))
                                            .px(px(10.0))
                                            .py(px(4.0))
                                            .rounded(px(6.0))
                                            .border_1()
                                            .border_color(theme.border_strong)
                                            .bg(theme.element_active)
                                            .text_size(px(12.0))
                                            .text_color(theme.text_primary)
                                            .child(format!("{} min", settings.refresh_interval_mins))
                                            .child(
                                                div()
                                                    .text_size(px(7.0))
                                                    .text_color(theme.text_muted)
                                                    .ml(px(1.0))
                                                    .child("▲▼"),
                                            ),
                                    ),
                            )
                            .child(Self::render_card_separator())
                            .child(
                                div()
                                    .flex()
                                    .items_start()
                                    .gap(px(10.0))
                                    .px(px(14.0))
                                    .py(px(10.0))
                                    .child(self.render_settings_checkbox(true, theme))
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
                                    ),
                            )
                            .child(Self::render_card_separator())
                            .child(
                                div()
                                    .flex()
                                    .items_start()
                                    .gap(px(10.0))
                                    .px(px(14.0))
                                    .py(px(10.0))
                                    .child(self.render_settings_checkbox(true, theme))
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
                            .child("Quit BananaTray")
                            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                cx.quit();
                            }),
                    ),
            )
    }
}
