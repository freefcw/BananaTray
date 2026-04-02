//! Copilot provider 的 Settings UI 渲染
//!
//! 支持交互式 Token 配置，匹配设计稿卡片样式。

use crate::app::persist_settings;
use crate::theme::Theme;
use crate::utils::platform::open_url;
use gpui::prelude::FluentBuilder;
use gpui::*;
use rust_i18n::t;

/// 渲染 Copilot 设置 UI（带交互，用于设置窗口）
/// 设计稿：深色卡片容器 → 标题+描述 → 状态徽章 → token 信息 → 操作按钮
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

    // ── 外层深色卡片容器 ──
    let mut card = div()
        .flex_col()
        .w_full()
        .rounded(px(12.0))
        .bg(theme.bg_card_inner)
        .border_1()
        .border_color(theme.border_strong)
        .px(px(20.0))
        .py(px(20.0))
        .gap(px(14.0));

    // ── 标题 ──
    card = card.child(
        div()
            .text_size(px(15.0))
            .font_weight(FontWeight::BOLD)
            .text_color(theme.text_primary)
            .child(t!("copilot.github_login").to_string()),
    );

    // ── 描述 ──
    card = card.child(
        div()
            .text_size(px(12.5))
            .line_height(relative(1.4))
            .text_color(theme.text_secondary)
            .py(px(4.0))
            .child(t!("copilot.requires_auth").to_string()),
    );

    // ── Token 状态区 ──
    if has_token {
        // 绿色状态行：✓ Token 已配置
        card = card.child(
            div()
                .w_full()
                .flex()
                .items_center()
                .gap(px(8.0))
                .px(px(14.0))
                .py(px(10.0))
                .rounded(px(8.0))
                .bg(hsla(145.0 / 360.0, 0.6, 0.3, 0.15)) // 半透明绿色背景
                .border_1()
                .border_color(hsla(145.0 / 360.0, 0.6, 0.4, 0.35)) // 绿色边框
                // ✓ 图标
                .child(
                    div()
                        .text_size(px(14.0))
                        .text_color(theme.status_success)
                        .child("✓"),
                )
                // 文字
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.status_success)
                        .child(t!("copilot.token_configured").to_string()),
                ),
        );

        // Token 来源信息 — 增加上下间距避免拥挤
        card = card.child(
            div()
                .py(px(6.0))
                .text_size(px(12.0))
                .text_color(theme.text_muted)
                .child(
                    t!(
                        "copilot.token_via",
                        masked = masked.unwrap_or_default(),
                        source = &status.source
                    )
                    .to_string(),
                ),
        );
    } else {
        // 未配置提示
        card = card.child(
            div()
                .text_size(px(12.0))
                .line_height(relative(1.5))
                .text_color(theme.text_muted)
                .child(t!("copilot.token_hint").to_string()),
        );
    }

    // ── 操作按钮 ──
    card = card.child(render_action_buttons(state.clone(), has_token, theme));

    card
}

fn render_action_buttons(
    state: std::rc::Rc<std::cell::RefCell<crate::app::AppState>>,
    has_token: bool,
    theme: &Theme,
) -> Div {
    let state_clear = state.clone();

    div()
        .flex()
        .gap(px(10.0))
        .mt(px(2.0))
        // ── "创建 GitHub Token" 按钮 — 紫色/蓝色圆角 ──
        .child(
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .px(px(16.0))
                .py(px(10.0))
                .rounded(px(8.0))
                .bg(theme.text_accent)
                .text_size(px(13.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.element_active)
                .cursor_pointer()
                .hover(|s| s.opacity(0.9))
                .child(t!("copilot.create_token").to_string())
                .on_mouse_down(MouseButton::Left, |_, _, _| {
                    open_url("https://github.com/settings/tokens/new?scopes=copilot");
                }),
        )
        // ── "清除 Token" 按钮 — 暗红色圆角（仅已有 token 时显示）──
        .when(has_token, |this| {
            this.child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .px(px(16.0))
                    .py(px(10.0))
                    .rounded(px(8.0))
                    .bg(theme.btn_danger_bg)
                    .border_1()
                    .border_color(hsla(0.0, 0.6, 0.4, 0.3))
                    .text_size(px(13.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.status_error)
                    .cursor_pointer()
                    .hover(|s| s.opacity(0.9))
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
