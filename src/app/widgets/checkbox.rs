use crate::theme::Theme;
use gpui::*;

/// Render a checkbox element.
/// Currently unused but kept for potential future use.
#[allow(dead_code)]
pub(crate) fn render_checkbox(checked: bool, size: Pixels, theme: &Theme) -> Div {
    let accent = theme.text.accent;
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
        .border_color(if checked { accent } else { theme.border.strong })
        .bg(if checked { accent } else { transparent_black() })
        .text_size(px(11.0))
        .font_weight(FontWeight::BOLD)
        .text_color(if checked {
            theme.element.active
        } else {
            transparent_black()
        })
        .child(if checked { "✓" } else { "" })
}
