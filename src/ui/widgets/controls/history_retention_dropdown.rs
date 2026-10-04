use super::dropdown::{render_dropdown_panel, render_dropdown_row, render_dropdown_trigger};
use crate::theme::Theme;
use gpui::{
    deferred, px, App, Deferred, Div, InteractiveElement, MouseButton, MouseDownEvent,
    ParentElement, Styled, Window,
};

const DROPDOWN_WIDTH: f32 = 168.0;

pub(crate) fn render_history_retention_dropdown<T, Toggle, Select>(
    label: String,
    open: bool,
    options: Vec<(String, T)>,
    current: &T,
    theme: &Theme,
    on_toggle: Toggle,
    on_select: Select,
) -> Div
where
    T: PartialEq + Clone + 'static,
    Toggle: Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    Select: Fn(T, &mut Window, &mut App) + Clone + 'static,
{
    let mut trigger = render_dropdown_trigger(label, open, DROPDOWN_WIDTH, true, true, theme)
        .cursor_pointer()
        .on_mouse_down(MouseButton::Left, on_toggle);

    if open {
        trigger = trigger.child(render_options(options, current, theme, on_select));
    }

    trigger
}

fn render_options<T, Select>(
    options: Vec<(String, T)>,
    current: &T,
    theme: &Theme,
    on_select: Select,
) -> Deferred
where
    T: PartialEq + Clone + 'static,
    Select: Fn(T, &mut Window, &mut App) + Clone + 'static,
{
    deferred(
        render_dropdown_panel(DROPDOWN_WIDTH, true, theme)
            .flex()
            .flex_col()
            .gap(px(2.0))
            .children(options.into_iter().map(move |(label, value)| {
                let is_active = &value == current;
                let on_select = on_select.clone();
                render_dropdown_row(label, is_active, true, theme).on_mouse_down(
                    MouseButton::Left,
                    move |_: &MouseDownEvent, window: &mut Window, cx: &mut App| {
                        on_select(value.clone(), window, cx);
                    },
                )
            })),
    )
    .with_priority(1)
}
