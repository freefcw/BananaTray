use crate::theme::Theme;
use gpui::*;

/// Render a checkbox element.
/// Currently unused but kept for potential future use.
#[allow(dead_code)]
pub(crate) fn render_checkbox(checked: bool, size: Pixels, theme: &Theme) -> Div {
    let blue: Hsla = rgb(0x007aff).into();
    div()
        .mt(px(1.0))
        .w(size)
        .h(size)
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .rounded(px(4.0))
        .border_1()
        .border_color(if checked { blue } else { theme.border_strong })
        .bg(if checked { blue } else { transparent_black() })
        .text_size(px(11.0))
        .font_weight(FontWeight::BOLD)
        .text_color(if checked {
            theme.element_active
        } else {
            transparent_black()
        })
        .child(if checked { "✓" } else { "" })
}
