use super::render_checkbox;
use crate::theme::Theme;
use gpui::*;

/// macOS grouped-style 白色圆角卡片
pub(crate) fn render_card() -> Div {
    div()
        .flex_col()
        .rounded(px(10.0))
        .bg(rgb(0xffffff))
        .overflow_hidden()
}

/// 卡片内部水平分隔线（左缩进）
pub(crate) fn render_card_separator() -> Div {
    div().h(px(0.5)).w_full().ml(px(14.0)).bg(rgb(0xe5e5ea))
}

/// 小号段落标签（如 "SYSTEM"、"USAGE"），12px muted
pub(crate) fn render_section_label(title: &str, theme: &Theme) -> Div {
    div()
        .text_size(px(12.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme.text_muted)
        .px(px(4.0))
        .pb(px(6.0))
        .child(title.to_string())
}

/// 详情区段标题（如 "Usage"、"Settings"），14px primary
pub(crate) fn render_detail_section_title(title: &str, theme: &Theme) -> Div {
    div()
        .text_size(px(14.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme.text_primary)
        .child(title.to_string())
}

/// 设置项行：checkbox + 标题 + 描述（不含事件处理，调用方通过 .on_mouse_down 添加）
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
                        .text_color(theme.text_secondary)
                        .child(description.to_string()),
                ),
        )
}

/// 键值信息行
pub(crate) fn render_info_row(label: &str, value: &str, theme: &Theme) -> Div {
    div()
        .flex()
        .items_center()
        .child(
            div()
                .w(px(70.0))
                .text_size(px(12.0))
                .text_color(theme.text_muted)
                .child(label.to_string()),
        )
        .child(
            div()
                .text_size(px(12.0))
                .text_color(theme.text_primary)
                .child(value.to_string()),
        )
}
