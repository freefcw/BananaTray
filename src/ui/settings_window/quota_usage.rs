use crate::theme::Theme;
use crate::ui::widgets::{render_dropdown_panel, render_dropdown_row, render_dropdown_trigger};
use gpui::{
    deferred, px, App, Deferred, Div, InteractiveElement, MouseButton, MouseDownEvent,
    ParentElement, Styled, Window,
};
use rust_i18n::t;
use std::rc::Rc;

const TRIGGER_WIDTH: f32 = 200.0;
const PANEL_WIDTH: f32 = 320.0;

const STEP_OPTIONS: &[u8] = &[0, 5, 10, 20];

fn step_label(step: u8) -> String {
    if step == 0 {
        t!("quota_usage.off").to_string()
    } else {
        t!("quota_usage.every", n = step).to_string()
    }
}

fn option_label(option: Option<u8>, inherited_step: Option<u8>) -> String {
    match option {
        Some(step) => step_label(step),
        None => match inherited_step {
            Some(inherited) => t!("quota_usage.inherit", step = step_label(inherited)).to_string(),
            None => t!("quota_usage.off").to_string(),
        },
    }
}

fn usage_options(selection: Option<u8>, inherited_step: Option<u8>) -> Vec<Option<u8>> {
    let mut options = Vec::with_capacity(STEP_OPTIONS.len() + 2);
    if inherited_step.is_some() {
        options.push(None);
    }
    options.extend(STEP_OPTIONS.iter().map(|step| Some(*step)));
    if let Some(step) = selection {
        if !STEP_OPTIONS.contains(&step) {
            options.push(Some(step));
        }
    }
    options
}

pub(super) fn render_quota_usage_dropdown(
    selection: Option<u8>,
    inherited_step: Option<u8>,
    is_open: bool,
    theme: &Theme,
    on_toggle: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    on_select: impl Fn(Option<u8>, &mut Window, &mut App) + 'static,
) -> Div {
    let mut trigger = render_dropdown_trigger(
        option_label(selection, inherited_step),
        is_open,
        TRIGGER_WIDTH,
        true,
        true,
        theme,
    )
    .cursor_pointer()
    .on_mouse_down(MouseButton::Left, on_toggle);

    if is_open {
        trigger = trigger.child(render_options(selection, inherited_step, theme, on_select));
    }

    trigger
}

fn render_options(
    selection: Option<u8>,
    inherited_step: Option<u8>,
    theme: &Theme,
    on_select: impl Fn(Option<u8>, &mut Window, &mut App) + 'static,
) -> Deferred {
    let on_select = Rc::new(on_select);

    deferred(
        render_dropdown_panel(PANEL_WIDTH, true, theme)
            .flex()
            .flex_col()
            .gap(px(2.0))
            .children(
                usage_options(selection, inherited_step)
                    .into_iter()
                    .map(move |option| {
                        let is_active = selection == option;
                        let label = option_label(option, inherited_step);
                        let on_select = on_select.clone();

                        render_dropdown_row(label, is_active, true, theme).on_mouse_down(
                            MouseButton::Left,
                            move |_: &MouseDownEvent, window: &mut Window, cx: &mut App| {
                                on_select(option, window, cx);
                            },
                        )
                    }),
            ),
    )
    .with_priority(1)
}
