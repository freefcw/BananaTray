use gpui::*;

use crate::models::{AppSettings, AppTheme};
use crate::theme::Theme;

const GENERAL_ICON: &str = "src/icons/settings.svg";
const PROVIDERS_ICON: &str = "src/icons/overview.svg";
const DISPLAY_ICON: &str = "src/icons/display.svg";
const ADVANCED_ICON: &str = "src/icons/advanced.svg";
const ABOUT_ICON: &str = "src/icons/about.svg";

// ============================================================================
// 设置面板
// ============================================================================

/// 设置面板：主题切换、刷新间隔、Provider 管理
#[derive(IntoElement)]
pub struct SettingsPanel {
    settings: AppSettings,
}

impl SettingsPanel {
    pub fn new(settings: AppSettings) -> Self {
        Self { settings }
    }
}

impl RenderOnce for SettingsPanel {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.global::<Theme>();
        let settings = &self.settings;
        let theme_label = match settings.theme {
            AppTheme::Dark => "Dark",
            AppTheme::Light => "Light",
        };

        div()
            .flex()
            .flex_col()
            .gap(px(14.0))
            .rounded(px(16.0))
            .bg(theme.bg_card)
            .border_1()
            .border_color(theme.border_subtle)
            .p(px(16.0))
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .flex_col()
                            .gap(px(3.0))
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme.text_accent)
                                    .child("BANANA CONTROL"),
                            )
                            .child(
                                div()
                                    .text_size(px(20.0))
                                    .font_weight(FontWeight::BOLD)
                                    .child("Settings"),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(theme.text_secondary)
                                    .child("Tune the tray app without turning it into a dashboard."),
                            ),
                    )
                    .child(
                        div()
                            .px(px(10.0))
                            .py(px(5.0))
                            .rounded_full()
                            .bg(theme.bg_subtle)
                            .border_1()
                            .border_color(theme.text_accent_soft)
                            .text_size(px(11.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_accent)
                            .child("Quiet defaults"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .rounded(px(16.0))
                    .bg(theme.bg_panel)
                    .border_1()
                    .border_color(theme.border_subtle)
                    .p(px(14.0))
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .items_start()
                            .child(
                                div()
                                    .flex_col()
                                    .gap(px(4.0))
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .text_color(theme.text_muted)
                                            .child("Workspace Snapshot"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(16.0))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(theme.text_primary)
                                            .child("Defaults tuned for a quiet tray app."),
                                    ),
                            )
                            .child(
                                div()
                                    .px(px(10.0))
                                    .py(px(6.0))
                                    .rounded_full()
                                    .bg(theme.bg_subtle)
                                    .border_1()
                                    .border_color(theme.text_accent_soft)
                                    .text_size(px(11.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme.text_accent)
                                    .child("Current"),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(8.0))
                            .child(self.render_snapshot_chip("Theme", theme_label, theme))
                            .child(self.render_snapshot_chip(
                                "Refresh",
                                &format!("{} min", settings.refresh_interval_mins),
                                theme,
                            ))
                            .child(self.render_snapshot_chip(
                                "Hotkey",
                                &settings.global_hotkey,
                                theme,
                            ))
                            .child(self.render_snapshot_chip(
                                "Hide",
                                if settings.auto_hide_window { "Auto" } else { "Manual" },
                                theme,
                            )),
                    ),
            )
            .child(
                div()
                    .flex()
                    .justify_between()
                    .gap(px(8.0))
                    .children(vec![
                        self.render_prefs_tab(GENERAL_ICON, "General", true, theme),
                        self.render_prefs_tab(PROVIDERS_ICON, "Providers", false, theme),
                        self.render_prefs_tab(DISPLAY_ICON, "Display", false, theme),
                        self.render_prefs_tab(ADVANCED_ICON, "Advanced", false, theme),
                        self.render_prefs_tab(ABOUT_ICON, "About", false, theme),
                    ]),
            )
            .child(self.render_section(
                "Window",
                vec![
                    self.render_checkbox_row(
                        "Start at Login",
                        "Automatically opens BananaTray when your Mac starts.",
                        false,
                        theme,
                    ),
                ],
                theme,
            ))
            .child(self.render_section(
                "Monitoring",
                vec![
                    self.render_checkbox_row(
                        "Compact tray summary",
                        &format!(
                            "Theme: {} · Hotkey: {} · Refresh every {} min",
                            theme_label,
                            settings.global_hotkey,
                            settings.refresh_interval_mins,
                        ),
                        true,
                        theme,
                    ),
                ],
                theme,
            ))
            .child(self.render_section(
                "Automation",
                vec![
                    self.render_select_row(
                        "Refresh cadence",
                        "How often BananaTray polls providers in the background.",
                        &format!("{} min", settings.refresh_interval_mins),
                        theme,
                    ),
                    self.render_checkbox_row(
                        "Check provider status",
                        "Polls status pages and surfaces incidents in the menu.",
                        true,
                        theme,
                    ),
                    self.render_checkbox_row(
                        "Session quota notifications",
                        "Notify when a 5-hour session hits 0% and when quota resets.",
                        true,
                        theme,
                    ),
                ],
                theme,
            ))
            .child(
                div()
                    .flex_col()
                    .gap(px(8.0))
                    .rounded(px(14.0))
                    .bg(theme.bg_card)
                    .border_1()
                    .border_color(theme.border_subtle)
                    .p(px(14.0))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_muted)
                            .child("Maintenance"),
                    )
                    .child(
                        div()
                            .text_size(px(13.0))
                            .line_height(relative(1.4))
                            .text_color(theme.text_secondary)
                            .child("Close the tray app completely and stop background quota monitoring."),
                    )
                    .child(
                        div()
                            .px(px(14.0))
                            .py(px(10.0))
                            .rounded(px(12.0))
                            .bg(theme.status_error)
                            .text_color(theme.element_active)
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Quit BananaTray"),
                    ),
            )
    }
}

impl SettingsPanel {
    fn render_section(&self, title: &str, rows: Vec<Div>, theme: &Theme) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .rounded(px(14.0))
            .bg(theme.bg_card)
            .border_1()
            .border_color(theme.border_subtle)
            .p(px(14.0))
            .child(
                div()
                    .text_size(px(11.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text_muted)
                    .child(title.to_string()),
            )
            .children(rows)
    }

    fn render_prefs_tab(
        &self,
        icon_path: &'static str,
        label: &str,
        active: bool,
        theme: &Theme,
    ) -> Div {
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(6.0))
            .flex_1()
            .py(px(10.0))
            .rounded(px(12.0))
            .bg(if active {
                theme.bg_card_active
            } else {
                theme.bg_panel
            })
            .border_1()
            .border_color(if active {
                theme.text_accent
            } else {
                theme.border_subtle
            })
            .child(
                div()
                    .w(px(24.0))
                    .h(px(24.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(7.0))
                    .border_1()
                    .border_color(if active {
                        theme.text_accent
                    } else {
                        theme.border_strong
                    })
                    .bg(if active {
                        theme.element_selected
                    } else {
                        theme.bg_subtle
                    })
                    .child(svg().path(icon_path).size(px(14.0)).text_color(if active {
                        theme.element_active
                    } else {
                        theme.text_secondary
                    })),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .font_weight(if active {
                        FontWeight::SEMIBOLD
                    } else {
                        FontWeight::NORMAL
                    })
                    .text_color(if active {
                        theme.element_active
                    } else {
                        theme.text_secondary
                    })
                    .child(label.to_string()),
            )
    }

    fn render_snapshot_chip(&self, label: &str, value: &str, theme: &Theme) -> Div {
        div()
            .flex_1()
            .flex_col()
            .gap(px(4.0))
            .px(px(10.0))
            .py(px(10.0))
            .rounded(px(12.0))
            .bg(theme.bg_subtle)
            .border_1()
            .border_color(theme.text_accent_soft)
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(theme.text_muted)
                    .child(label.to_string()),
            )
            .child(
                div()
                    .text_size(px(13.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text_primary)
                    .child(value.to_string()),
            )
    }

    fn render_checkbox_row(
        &self,
        title: &str,
        description: &str,
        checked: bool,
        theme: &Theme,
    ) -> Div {
        div()
            .flex()
            .justify_between()
            .gap(px(10.0))
            .rounded(px(12.0))
            .bg(theme.bg_panel)
            .border_1()
            .border_color(if checked {
                theme.text_accent_soft
            } else {
                theme.border_subtle
            })
            .px(px(10.0))
            .py(px(10.0))
            .child(
                div()
                    .flex()
                    .gap(px(10.0))
                    .flex_1()
                    .child(self.render_checkbox(checked, theme))
                    .child(
                        div()
                            .flex_col()
                            .gap(px(3.0))
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(title.to_string()),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(theme.text_secondary)
                                    .child(description.to_string()),
                            ),
                    ),
            )
            .child(
                div()
                    .px(px(8.0))
                    .py(px(5.0))
                    .rounded(px(9.0))
                    .bg(theme.bg_subtle)
                    .border_1()
                    .border_color(if checked {
                        theme.text_accent
                    } else {
                        theme.border_strong
                    })
                    .text_size(px(11.0))
                    .text_color(if checked {
                        theme.text_accent
                    } else {
                        theme.text_muted
                    })
                    .child(if checked { "On" } else { "Off" }),
            )
    }

    fn render_select_row(&self, title: &str, description: &str, value: &str, theme: &Theme) -> Div {
        div()
            .flex()
            .justify_between()
            .items_center()
            .rounded(px(12.0))
            .bg(theme.bg_panel)
            .border_1()
            .border_color(theme.text_accent_soft)
            .px(px(10.0))
            .py(px(10.0))
            .child(
                div()
                    .flex_col()
                    .gap(px(3.0))
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title.to_string()),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme.text_secondary)
                            .child(description.to_string()),
                    ),
            )
            .child(
                div()
                    .px(px(12.0))
                    .py(px(6.0))
                    .rounded(px(10.0))
                    .bg(theme.bg_subtle)
                    .border_1()
                    .border_color(theme.text_accent)
                    .text_size(px(13.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(value.to_string()),
            )
    }

    fn render_checkbox(&self, checked: bool, theme: &Theme) -> Div {
        div()
            .mt(px(2.0))
            .w(px(18.0))
            .h(px(18.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(5.0))
            .bg(if checked {
                theme.element_selected
            } else {
                theme.bg_subtle
            })
            .border_1()
            .border_color(if checked {
                theme.text_accent
            } else {
                theme.border_strong
            })
            .text_size(px(10.0))
            .font_weight(FontWeight::BOLD)
            .text_color(if checked {
                theme.element_active
            } else {
                theme.text_muted
            })
            .child(if checked { "OK" } else { "  " })
    }
}
