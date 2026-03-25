use super::SettingsView;
use crate::models::AppSettings;
use crate::theme::Theme;
use gpui::*;

impl SettingsView {
    /// Render Providers settings tab
    pub(super) fn render_providers_tab(&self, settings: &AppSettings, theme: &Theme) -> Div {
        let github_token = settings.providers.github_token.clone().unwrap_or_default();
        let has_token = !github_token.is_empty();

        div()
            .flex_col()
            .flex_1()
            .px(px(16.0))
            .pt(px(16.0))
            .pb(px(20.0))
            // ═══════ COPILOT ═══════
            .child(
                div()
                    .flex_col()
                    .child(Self::render_section_label("GITHUB COPILOT", theme))
                    .child(
                        Self::render_card()
                            // GitHub Token
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
                                                    .child("GitHub Token"),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(12.5))
                                                    .line_height(relative(1.4))
                                                    .text_color(theme.text_secondary)
                                                    .child("Classic PAT with 'copilot' scope. Auto-detects plan & quota."),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_shrink_0()
                                            .items_center()
                                            .ml(px(12.0))
                                            .px(px(10.0))
                                            .py(px(4.0))
                                            .rounded(px(6.0))
                                            .border_1()
                                            .border_color(theme.border_strong)
                                            .bg(theme.element_active)
                                            .text_size(px(12.0))
                                            .text_color(if github_token.is_empty() {
                                                theme.text_muted
                                            } else {
                                                theme.text_primary
                                            })
                                            .child(if github_token.is_empty() {
                                                "Not set".to_string()
                                            } else {
                                                format!("{}...", &github_token[..8.min(github_token.len())])
                                            }),
                                    ),
                            )
                            .child(Self::render_card_separator())
                            // Status
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
                                            .child(
                                                div()
                                                    .text_size(px(13.0))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .child("Status"),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(12.5))
                                                    .line_height(relative(1.4))
                                                    .text_color(theme.text_secondary)
                                                    .child(if has_token {
                                                        "Token configured. Copilot quota will be auto-detected."
                                                    } else {
                                                        "Set your GitHub token to enable Copilot monitoring."
                                                    }),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .px(px(8.0))
                                            .py(px(4.0))
                                            .rounded(px(6.0))
                                            .bg(if has_token {
                                                theme.status_success
                                            } else {
                                                theme.status_warning
                                            })
                                            .text_size(px(11.0))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(theme.element_active)
                                            .child(if has_token { "Ready" } else { "Not Configured" }),
                                    ),
                            ),
                    ),
            )
            // ═══════ CONFIG FILE INFO ═══════
            .child(
                div()
                    .flex_col()
                    .mt(px(12.0))
                    .child(Self::render_section_label("CONFIGURATION", theme))
                    .child(
                        Self::render_card()
                            .child(
                                div()
                                    .flex()
                                    .items_start()
                                    .gap(px(10.0))
                                    .px(px(14.0))
                                    .py(px(10.0))
                                    .child(
                                        div()
                                            .flex_col()
                                            .gap(px(4.0))
                                            .flex_1()
                                            .child(
                                                div()
                                                    .text_size(px(13.0))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .child("Config File Location"),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(11.5))
                                                    .text_color(theme.text_muted)
                                                    .child("~/Library/Application Support/BananaTray/settings.json"),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(12.5))
                                                    .line_height(relative(1.4))
                                                    .text_color(theme.text_secondary)
                                                    .mt(px(4.0))
                                                    .child("Click 'Edit Config' to open the file in your default editor."),
                                            ),
                                    ),
                            )
                            .child(Self::render_card_separator())
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .px(px(14.0))
                                    .py(px(10.0))
                                    .child(
                                        div()
                                            .text_size(px(12.5))
                                            .text_color(theme.text_secondary)
                                            .child("Example format:"),
                                    )
                                    .child(
                                        div()
                                            .px(px(12.0))
                                            .py(px(6.0))
                                            .rounded(px(8.0))
                                            .bg(theme.text_accent)
                                            .text_size(px(12.0))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(theme.element_active)
                                            .cursor_pointer()
                                            .child("Edit Config")
                                            .on_mouse_down(MouseButton::Left, |_, _, _| {
                                                let path = dirs::config_dir()
                                                    .unwrap_or_else(|| std::path::PathBuf::from("."))
                                                    .join("BananaTray")
                                                    .join("settings.json");
                                                if let Some(parent) = path.parent() {
                                                    let _ = std::fs::create_dir_all(parent);
                                                }
                                                let _ = std::process::Command::new("open")
                                                    .arg(&path)
                                                    .spawn();
                                            }),
                                    ),
                            ),
                    ),
            )
            // ═══════ ENV VARIABLES INFO ═══════
            .child(
                div()
                    .flex_col()
                    .mt(px(12.0))
                    .child(Self::render_section_label("ALTERNATIVE", theme))
                    .child(
                        Self::render_card()
                            .child(
                                div()
                                    .flex()
                                    .items_start()
                                    .gap(px(10.0))
                                    .px(px(14.0))
                                    .py(px(10.0))
                                    .child(
                                        div()
                                            .flex_col()
                                            .gap(px(4.0))
                                            .child(
                                                div()
                                                    .text_size(px(13.0))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .child("Environment Variables"),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(12.5))
                                                    .line_height(relative(1.4))
                                                    .text_color(theme.text_secondary)
                                                    .child("You can also set GITHUB_USERNAME and GITHUB_TOKEN environment variables instead of using the config file."),
                                            ),
                                    ),
                            ),
                    ),
            )
    }
}
