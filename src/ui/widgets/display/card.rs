use crate::theme::Theme;
use gpui::{div, px, Div, FontWeight, ParentElement, Styled};

/// 详情区段标题（如 "Usage"、"Settings"），14px primary
pub(crate) fn render_detail_section_title(title: &str, theme: &Theme) -> Div {
    div()
        .text_size(px(14.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme.text.primary)
        .child(title.to_string())
}
