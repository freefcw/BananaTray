use gpui::{svg, Hsla, IntoElement, Pixels, SharedString, Styled};

pub(crate) fn render_svg_icon(
    path: impl Into<SharedString>,
    size: Pixels,
    color: Hsla,
) -> impl IntoElement {
    svg().path(path).size(size).text_color(color)
}
