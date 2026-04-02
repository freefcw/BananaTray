use crate::theme::Theme;
use gpui::*;

pub(crate) fn render_toggle_switch(
    enabled: bool,
    width: Pixels,
    height: Pixels,
    knob_size: Pixels,
    theme: &Theme,
) -> Div {
    let travel = width - knob_size - px(4.0);
    div()
        .w(width)
        .h(height)
        .flex()
        .items_center()
        .rounded_full()
        .px(px(2.0))
        .bg(if enabled {
            theme.element_selected
        } else {
            theme.bg_subtle
        })
        .border_1()
        .border_color(if enabled {
            theme.element_selected
        } else {
            theme.border_strong
        })
        .child(
            div()
                .w(knob_size)
                .h(knob_size)
                .rounded_full()
                .bg(theme.element_active)
                .ml(if enabled { travel } else { px(0.0) }),
        )
}
