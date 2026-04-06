use crate::theme::Theme;
use gpui::*;

pub(crate) fn render_svg_icon(
    path: impl Into<SharedString>,
    size: Pixels,
    color: Hsla,
) -> impl IntoElement {
    svg().path(path).size(size).text_color(color)
}

pub(crate) fn render_footer_glyph(
    icon_path: impl Into<SharedString>,
    theme: &Theme,
) -> impl IntoElement {
    div()
        .w(px(18.0))
        .h(px(18.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.0))
        .border_1()
        .border_color(theme.text.accent_soft)
        .bg(theme.bg.subtle)
        .child(render_svg_icon(icon_path, px(11.0), theme.text.accent))
}
