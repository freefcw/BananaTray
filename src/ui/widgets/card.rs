use super::render_checkbox;
use crate::theme::Theme;
use gpui::{
    div, px, relative, App, Div, FontWeight, InteractiveElement, MouseButton, MouseDownEvent,
    ParentElement, Styled, Window,
};

/// macOS grouped-style 圆角卡片
#[allow(dead_code)]
pub(crate) fn render_card(theme: &Theme) -> Div {
    div()
        .flex_col()
        .rounded(px(10.0))
        .bg(theme.bg.panel)
        .overflow_hidden()
}

/// 卡片内部水平分隔线（左缩进）
#[allow(dead_code)]
pub(crate) fn render_card_separator(theme: &Theme) -> Div {
    div()
        .h(px(0.5))
        .w_full()
        .ml(px(14.0))
        .bg(theme.status.progress_track)
}

/// 小号段落标签（如 "SYSTEM"、"USAGE"），12px muted
#[allow(dead_code)]
pub(crate) fn render_section_label(title: &str, theme: &Theme) -> Div {
    div()
        .text_size(px(12.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme.text.muted)
        .px(px(4.0))
        .pb(px(6.0))
        .child(title.to_string())
}

/// 详情区段标题（如 "Usage"、"Settings"），14px primary
pub(crate) fn render_detail_section_title(title: &str, theme: &Theme) -> Div {
    div()
        .text_size(px(14.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme.text.primary)
        .child(title.to_string())
}

/// 设置项行：checkbox + 标题 + 描述（不含事件处理，调用方通过 .on_mouse_down 添加）
/// Render a checkbox row with title and description.
/// Currently unused but kept for potential future use.
#[allow(dead_code)]
pub(crate) fn render_checkbox_row(
    title: &str,
    description: &str,
    checked: bool,
    theme: &Theme,
) -> Div {
    div()
        .flex()
        .items_start()
        .gap(px(10.0))
        .px(px(14.0))
        .py(px(10.0))
        .cursor_pointer()
        .child(render_checkbox(checked, px(18.0), theme))
        .child(
            div()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::MEDIUM)
                        .child(title.to_string()),
                )
                .child(
                    div()
                        .text_size(px(12.5))
                        .line_height(relative(1.4))
                        .text_color(theme.text.secondary)
                        .child(description.to_string()),
                ),
        )
}

/// Render a row with a toggle switch on the right.
/// Only the switch is clickable, not the entire row.
#[allow(dead_code)]
pub(crate) fn render_switch_row<F>(
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
        .justify_between()
        .gap(px(12.0))
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
                        .child(title.to_string()),
                )
                .child(
                    div()
                        .text_size(px(12.5))
                        .line_height(relative(1.4))
                        .text_color(theme.text.secondary)
                        .child(description.to_string()),
                ),
        )
        .child(
            super::render_toggle_switch(enabled, px(44.0), px(24.0), px(18.0), theme)
                .cursor_pointer()
                .on_mouse_down(MouseButton::Left, on_click),
        )
}

/// 键值信息行
#[allow(dead_code)]
pub(crate) fn render_info_row(label: &str, value: &str, theme: &Theme) -> Div {
    div()
        .flex()
        .items_center()
        .child(
            div()
                .w(px(70.0))
                .text_size(px(12.0))
                .text_color(theme.text.muted)
                .child(label.to_string()),
        )
        .child(
            div()
                .text_size(px(12.0))
                .text_color(theme.text.primary)
                .child(value.to_string()),
        )
}
