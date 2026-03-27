//! Copilot provider 的 Settings UI 渲染
//!
//! 纯 UI 渲染，不依赖 AppState，不产生副作用。

use super::CopilotTokenStatus;
use crate::theme::Theme;
use gpui::*;

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
                .child("GitHub Login"),
        )
        .child(
            div()
                .text_size(px(12.0))
                .line_height(relative(1.4))
                .text_color(theme.text_secondary)
                .child("Requires authentication via GitHub Token."),
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
                        .child("Token configured"),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(theme.text_muted)
                        .child(format!(
                            "{} · via {}",
                            masked.unwrap_or_default(),
                            status.source
                        )),
                )
        } else {
            div()
                .flex_col()
                .gap(px(6.0))
                .child(
                    div()
                        .text_size(px(11.5))
                        .text_color(theme.text_muted)
                        .child("Set token via config file or GITHUB_TOKEN env var"),
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
                        .child("Sign in with GitHub")
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
