use super::SettingsView;
use crate::app::widgets::{render_svg_icon, render_toggle_switch};
use crate::theme::Theme;
use gpui::*;

// ============================================================================
// 设计稿风格的段落标题和卡片
// ============================================================================

/// 段落标题（如 "SYSTEM"、"AUTOMATION"）— 大写、小号、间距
pub(super) fn render_section_header(title: &str, theme: &Theme) -> Div {
    div()
        .text_size(px(11.0))
        .font_weight(FontWeight::BOLD)
        .text_color(theme.text_muted)
        .px(px(4.0))
        .pt(px(16.0))
        .pb(px(8.0))
        .child(title.to_uppercase())
}

/// 深色卡片容器（与设计稿匹配的暗色圆角卡片）
pub(super) fn render_dark_card(theme: &Theme) -> Div {
    div()
        .flex_col()
        .w_full()
        .rounded(px(14.0))
        .bg(theme.bg_card)
        .border_1()
        .border_color(theme.border_subtle)
        .overflow_hidden()
}

/// 卡片内分隔线
pub(super) fn render_divider(theme: &Theme) -> Div {
    div().h(px(0.5)).w_full().bg(theme.border_subtle)
}

// ============================================================================
// 带彩色圆形图标的设置行（设计稿核心组件）
// ============================================================================

impl SettingsView {
    /// 渲染带彩色圆形背景图标的开关行
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_icon_switch_row<F>(
        icon_path: &'static str,
        icon_color: Hsla,
        icon_bg: Hsla,
        title: &str,
        description: &str,
        enabled: bool,
        theme: &Theme,
        on_click: F,
    ) -> Div
    where
        F: Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    {
        div()
            .flex()
            .items_center()
            .gap(px(12.0))
            .px(px(14.0))
            .py(px(12.0))
            // 彩色圆形图标
            .child(
                div()
                    .w(px(36.0))
                    .h(px(36.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .bg(icon_bg)
                    .flex_shrink_0()
                    .child(render_svg_icon(icon_path, px(18.0), icon_color)),
            )
            // 标题 + 描述
            .child(
                div()
                    .flex_col()
                    .flex_1()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_primary)
                            .child(title.to_string()),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme.text_muted)
                            .child(description.to_string()),
                    ),
            )
            // 开关
            .child(
                render_toggle_switch(enabled, px(44.0), px(24.0), px(18.0), theme)
                    .flex_shrink_0()
                    .cursor_pointer()
                    .on_mouse_down(MouseButton::Left, on_click),
            )
    }

    /// 渲染带彩色圆形背景图标的下拉行
    pub(super) fn render_icon_dropdown_row(
        icon_path: &'static str,
        icon_color: Hsla,
        icon_bg: Hsla,
        title: &str,
        description: &str,
        theme: &Theme,
        dropdown: Div,
    ) -> Div {
        div()
            .flex()
            .items_center()
            .gap(px(12.0))
            .px(px(14.0))
            .py(px(12.0))
            // 彩色圆形图标
            .child(
                div()
                    .w(px(36.0))
                    .h(px(36.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .bg(icon_bg)
                    .flex_shrink_0()
                    .child(render_svg_icon(icon_path, px(18.0), icon_color)),
            )
            // 标题 + 描述
            .child(
                div()
                    .flex_col()
                    .flex_1()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_primary)
                            .child(title.to_string()),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme.text_muted)
                            .child(description.to_string()),
                    ),
            )
            // 下拉控件
            .child(dropdown)
    }
}
