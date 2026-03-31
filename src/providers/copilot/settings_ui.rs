//! Copilot provider 的 Settings UI 渲染
//!
//! 支持交互式 Token 配置。

use crate::app::persist_settings;
use crate::theme::Theme;
use gpui::prelude::FluentBuilder;
use gpui::*;
use rust_i18n::t;

/// 打开外部 URL
fn open_url(url: &str) {
    let cmd = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "linux") {
        "xdg-open"
    } else {
        "start"
    };
    let _ = std::process::Command::new(cmd).arg(url).spawn();
}

/// 渲染 Copilot 设置 UI（带交互，用于设置窗口）
pub fn render_settings_interactive(
    state: std::rc::Rc<std::cell::RefCell<crate::app::AppState>>,
    theme: &Theme,
) -> Div {
    // 获取当前 token 状态
    let settings = state.borrow().settings.clone();
    let mem_token = settings.providers.github_token.as_deref();
    let status = super::resolve_token(mem_token);

    let has_token = status.token.is_some();
    let masked = status.masked();

    div()
        .flex_col()
        .gap(px(12.0))
        // ── 标题和描述 ──
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
        // ── 当前状态 ──
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
            div().flex_col().gap(px(6.0)).child(
                div()
                    .text_size(px(11.5))
                    .text_color(theme.text_muted)
                    .child(t!("copilot.token_hint").to_string()),
            )
        })
        // ── 操作按钮 ──
        .child(render_action_buttons(state.clone(), has_token, theme))
}

fn render_action_buttons(
    state: std::rc::Rc<std::cell::RefCell<crate::app::AppState>>,
    has_token: bool,
    theme: &Theme,
) -> Div {
    let state_clear = state.clone();

    div()
        .flex()
        .gap(px(8.0))
        .mt(px(4.0))
        // ── "创建 GitHub Token" 按钮 ──
        .child(
            div()
                .px(px(12.0))
                .py(px(8.0))
                .rounded(px(8.0))
                .bg(theme.text_accent)
                .text_size(px(12.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.element_active)
                .cursor_pointer()
                .child(t!("copilot.create_token").to_string())
                .on_mouse_down(MouseButton::Left, |_, _, _| {
                    open_url("https://github.com/settings/tokens/new?scopes=copilot");
                }),
        )
        // ── "清除 Token" 按钮（已有 token 时显示）──
        .when(has_token, |this| {
            this.child(
                div()
                    .px(px(12.0))
                    .py(px(8.0))
                    .rounded(px(8.0))
                    .bg(theme.status_error)
                    .text_size(px(12.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.element_active)
                    .cursor_pointer()
                    .child(t!("copilot.clear_token").to_string())
                    .on_mouse_down(MouseButton::Left, move |_, window, _| {
                        let mut s = state_clear.borrow_mut();
                        s.settings.providers.github_token = None;
                        let settings = s.settings.clone();
                        drop(s);
                        persist_settings(&settings);
                        window.refresh();
                    }),
            )
        })
}
