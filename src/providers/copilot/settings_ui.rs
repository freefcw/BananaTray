//! Copilot provider 的 Settings UI 渲染
//!
//! 纯 UI 渲染，不依赖 AppState，不产生副作用。

use super::CopilotTokenStatus;
use crate::theme::Theme;
use gpui::*;
use rust_i18n::t;

/// 渲染 Copilot 特有的设置面板
pub fn render_settings(status: &CopilotTokenStatus, theme: &Theme) -> Div {
    let has_token = status.token.is_some();
    let masked = status.masked();

    div()
        .flex_col()
        .gap(px(8.0))
        .child(
            div()
                .text_size(px(13.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text_primary)
                .child(t!("copilot.github_login").to_string()),
        )
        .child(
            div()
                .text_size(px(12.0))
                .line_height(relative(1.4))
                .text_color(theme.text_secondary)
                .child(t!("copilot.requires_auth").to_string()),
        )
        .child(if has_token {
            div()
                .flex_col()
                .gap(px(6.0))
                .child(
                    div()
                        .px(px(8.0))
                        .py(px(4.0))
                        .rounded(px(6.0))
                        .bg(theme.status_success)
                        .text_size(px(11.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.element_active)
                        .child(t!("copilot.token_configured").to_string()),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(theme.text_muted)
                        .child(
                            t!(
                                "copilot.token_via",
                                masked = masked.unwrap_or_default(),
                                source = &status.source
                            )
                            .to_string(),
                        ),
                )
        } else {
            div()
                .flex_col()
                .gap(px(6.0))
                .child(
                    div()
                        .text_size(px(11.5))
                        .text_color(theme.text_muted)
                        .child(t!("copilot.token_hint").to_string()),
                )
                .child(
                    div()
                        .w_full()
                        .py(px(8.0))
                        .rounded(px(8.0))
                        .bg(theme.text_primary)
                        .text_size(px(12.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.element_active)
                        .cursor_pointer()
                        .flex()
                        .justify_center()
                        .child(t!("copilot.sign_in").to_string())
                        .on_mouse_down(MouseButton::Left, |_, _, _| {
                            let path = crate::settings_store::config_path();
                            if let Some(parent) = path.parent() {
                                let _ = std::fs::create_dir_all(parent);
                            }
                            let cmd = if cfg!(target_os = "linux") {
                                "xdg-open"
                            } else {
                                "open"
                            };
                            let _ = std::process::Command::new(cmd).arg(&path).spawn();
                        }),
                )
        })
}
