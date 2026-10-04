use super::dropdown::{
    render_dropdown_panel, render_dropdown_row, render_dropdown_trigger, DROPDOWN_TRIGGER_HEIGHT,
};
use crate::theme::Theme;
use gpui::{
    deferred, div, px, App, Deferred, Div, InteractiveElement, MouseButton, MouseDownEvent,
    ParentElement, Styled, Window,
};

const STANDARD_WIDTH: f32 = 168.0;
const COMPACT_WIDTH: f32 = 120.0;
const COMPACT_HEIGHT: f32 = 24.0;

pub(crate) struct HistoryRetentionMenu {
    pub open: bool,
    pub compact: bool,
}

pub(crate) fn render_history_retention_dropdown<T, Toggle, Select>(
    label: String,
    menu: HistoryRetentionMenu,
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
    let HistoryRetentionMenu { open, compact } = menu;
    let width = if compact {
        COMPACT_WIDTH
    } else {
        STANDARD_WIDTH
    };
    let height = if compact {
        COMPACT_HEIGHT
    } else {
        DROPDOWN_TRIGGER_HEIGHT
    };
    let mut trigger = if compact {
        render_compact_trigger(label, open, theme)
    } else {
        render_dropdown_trigger(label, open, width, true, true, theme)
    };
    trigger = trigger
        .cursor_pointer()
        .on_mouse_down(MouseButton::Left, on_toggle);

    if open {
        trigger = trigger.child(render_options(
            options, current, width, height, theme, on_select,
        ));
    }

    trigger
}

fn render_compact_trigger(label: String, is_open: bool, theme: &Theme) -> Div {
    div()
        .relative()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_between()
        .h(px(COMPACT_HEIGHT))
        .w(px(COMPACT_WIDTH))
        .gap(px(4.0))
        .px(px(8.0))
        .rounded(px(6.0))
        .bg(theme.bg.subtle)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_size(px(12.0))
                .text_color(theme.text.secondary)
                .child(label),
        )
        .child(
            div()
                .text_size(px(9.0))
                .text_color(theme.text.muted)
                .child(if is_open { "▲" } else { "▼" }),
        )
}

fn render_options<T, Select>(
    options: Vec<(String, T)>,
    current: &T,
    width: f32,
    anchor_top: f32,
    theme: &Theme,
    on_select: Select,
) -> Deferred
where
    T: PartialEq + Clone + 'static,
    Select: Fn(T, &mut Window, &mut App) + Clone + 'static,
{
    deferred(
        render_dropdown_panel(width, anchor_top, true, theme)
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
