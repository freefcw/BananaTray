use super::SettingsView;
use crate::app::widgets::{render_icon_row, render_toggle_switch};
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
        .text_color(theme.text.muted)
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
        .bg(theme.bg.card)
        .border_1()
        .border_color(theme.border.subtle)
        .overflow_hidden()
}

/// 卡片内分隔线
pub(super) fn render_divider(theme: &Theme) -> Div {
    div().h(px(0.5)).w_full().bg(theme.border.subtle)
}

// ============================================================================
// 带彩色圆形图标的设置行（基于公共 render_icon_row 组件）
// ============================================================================

impl SettingsView {
    /// 渲染带彩色圆形背景图标的开关行
    /// 快捷函数：render_icon_row + render_toggle_switch 组合
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
        render_icon_row(
            icon_path,
            icon_color,
            icon_bg,
            title,
            description,
            theme,
            render_toggle_switch(enabled, px(44.0), px(24.0), px(18.0), theme)
                .flex_shrink_0()
                .cursor_pointer()
                .on_mouse_down(MouseButton::Left, on_click),
        )
    }

    /// 渲染带彩色圆形背景图标的下拉行
    /// 快捷函数：render_icon_row + 自定义下拉控件 组合
    pub(super) fn render_icon_dropdown_row(
        icon_path: &'static str,
        icon_color: Hsla,
        icon_bg: Hsla,
        title: &str,
        description: &str,
        theme: &Theme,
        dropdown: Div,
    ) -> Div {
        render_icon_row(
            icon_path,
            icon_color,
            icon_bg,
            title,
            description,
            theme,
            dropdown,
        )
    }
}
