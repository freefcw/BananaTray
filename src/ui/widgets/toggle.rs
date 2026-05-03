use crate::theme::Theme;
use gpui::{div, px, Div, ParentElement, Pixels, Styled};

pub(crate) fn render_toggle_switch(
    enabled: bool,
    width: Pixels,
    height: Pixels,
    knob_size: Pixels,
    theme: &Theme,
) -> Div {
    let track_bg = if enabled {
        theme.element.selected
    } else {
        theme.bg.subtle
    };
    let track_border = if enabled {
        theme.element.selected
    } else {
        theme.border.strong
    };

    let mut track = div()
        .flex_none()
        .w(width)
        .h(height)
        .flex()
        .items_center()
        .px(px(2.0))
        .rounded_full()
        .overflow_hidden()
        .bg(track_bg)
        .border_1()
        .border_color(track_border);

    track = if enabled {
        track.justify_end()
    } else {
        track.justify_start()
    };

    track.child(
        div()
            .flex_none()
            .w(knob_size)
            .h(knob_size)
            .rounded_full()
            .bg(theme.element.active),
    )
}
