use crate::theme::Theme;
use gpui::{div, hsla, px, relative, Div, FontWeight, ParentElement, Styled};

/// 详情区段标题（如 "Usage"、"Settings"），14px primary
pub(crate) fn render_detail_section_title(title: &str, theme: &Theme) -> Div {
    div()
        .text_size(px(14.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme.text.primary)
        .child(title.to_string())
}

/// 详情区段空态卡片：与配额列表同一套圆角/描边，避免裸文本掉在标题下。
pub(crate) fn render_detail_empty_card(message: &str, theme: &Theme) -> Div {
    div()
        .mt(px(8.0))
        .px(px(12.0))
        .py(px(12.0))
        .rounded(px(10.0))
        .bg(theme.bg.card)
        .border_1()
        .border_color(theme.border.subtle)
        .child(
            div()
                .text_size(px(12.5))
                .line_height(relative(1.45))
                .text_color(theme.text.muted)
                .child(message.to_string()),
        )
}

/// 详情区段失败卡片：淡红色底 + 左侧色条，标题与细节分层。
pub(crate) fn render_detail_error_card(title: &str, message: &str, theme: &Theme) -> Div {
    let error = theme.status.error;
    div()
        .mt(px(8.0))
        .flex()
        .rounded(px(10.0))
        .bg(hsla(error.h, error.s, error.l, 0.10))
        .border_1()
        .border_color(hsla(error.h, error.s, error.l, 0.28))
        .overflow_hidden()
        .child(div().w(px(3.0)).flex_shrink_0().bg(error))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .flex_col()
                .gap(px(4.0))
                .px(px(12.0))
                .py(px(10.0))
                .child(
                    div()
                        .text_size(px(12.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(error)
                        .child(title.to_string()),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .line_height(relative(1.45))
                        .text_color(theme.text.secondary)
                        .child(message.to_string()),
                ),
        )
}
