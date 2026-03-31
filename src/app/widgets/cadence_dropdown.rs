use crate::app::{persist_settings, AppState};
use crate::theme::Theme;
use gpui::prelude::FluentBuilder;
use gpui::*;
use rust_i18n::t;
use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

/// Available refresh cadence options (None = Manual, Some(mins) = Auto)
const REFRESH_OPTIONS: &[Option<u64>] = &[
    None,
    Some(1),
    Some(2),
    Some(3),
    Some(5),
    Some(10),
    Some(15),
    Some(30),
];

// Layout constants
const TRIGGER_PADDING_X: f32 = 10.0;
const TRIGGER_PADDING_Y: f32 = 5.0;
const TRIGGER_RADIUS: f32 = 6.0;
const TRIGGER_GAP: f32 = 4.0;
const TRIGGER_MARGIN_LEFT: f32 = 12.0;
const DROPDOWN_TOP_OFFSET: f32 = 32.0;
const DROPDOWN_MIN_WIDTH: f32 = 100.0;
const DROPDOWN_MAX_HEIGHT: f32 = 180.0;
const DROPDOWN_RADIUS: f32 = 8.0;
const OPTION_PADDING_X: f32 = 12.0;
const OPTION_PADDING_Y: f32 = 7.0;
const FONT_SIZE_TRIGGER: f32 = 12.0;
const FONT_SIZE_ARROW: f32 = 10.0;
const FONT_SIZE_OPTION: f32 = 12.5;
const FONT_SIZE_CHECK: f32 = 11.0;
const FONT_SIZE_LABEL: f32 = 13.0;
const FONT_SIZE_DESC: f32 = 12.5;
const ROW_PADDING_X: f32 = 14.0;
const ROW_PADDING_Y: f32 = 10.0;
const LABEL_GAP: f32 = 2.0;

/// Format a cadence option for display
fn format_cadence(mins: Option<u64>) -> String {
    match mins {
        None => t!("cadence.manual").to_string(),
        Some(1) => t!("cadence.1_minute").to_string(),
        Some(m) => t!("cadence.n_minutes", n = m).to_string(),
    }
}

/// Render a cadence dropdown row (collapsed trigger + optional expanded option list).
///
/// Returns a `Div` that can be placed inside a card.
pub(crate) fn render_cadence_dropdown(
    state: &Rc<RefCell<AppState>>,
    cadence_mins: Option<u64>,
    theme: &Theme,
) -> Div {
    let trigger = render_trigger_button(state, cadence_mins, theme);

    div()
        .flex()
        .items_center()
        .justify_between()
        .px(px(ROW_PADDING_X))
        .py(px(ROW_PADDING_Y))
        .child(render_cadence_label(theme))
        .child(trigger)
}

/// Left-side label with title and description.
fn render_cadence_label(theme: &Theme) -> Div {
    div()
        .flex_col()
        .gap(px(LABEL_GAP))
        .flex_1()
        .child(
            div()
                .text_size(px(FONT_SIZE_LABEL))
                .font_weight(FontWeight::MEDIUM)
                .child(t!("settings.refresh_cadence").to_string()),
        )
        .child(
            div()
                .text_size(px(FONT_SIZE_DESC))
                .line_height(relative(1.4))
                .text_color(theme.text_secondary)
                .child(t!("settings.refresh_cadence.desc").to_string()),
        )
}

/// Right-side trigger button that toggles the dropdown.
fn render_trigger_button(
    state: &Rc<RefCell<AppState>>,
    cadence_mins: Option<u64>,
    theme: &Theme,
) -> Div {
    let dropdown_open = state.borrow().settings_ui.cadence_dropdown_open;
    let toggle_state = state.clone();

    let mut trigger = div()
        .relative()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(TRIGGER_GAP))
        .ml(px(TRIGGER_MARGIN_LEFT))
        .px(px(TRIGGER_PADDING_X))
        .py(px(TRIGGER_PADDING_Y))
        .rounded(px(TRIGGER_RADIUS))
        .bg(theme.bg_subtle)
        .border_1()
        .border_color(if dropdown_open {
            theme.element_selected
        } else {
            theme.border_strong
        })
        .cursor_pointer()
        .child(
            div()
                .text_size(px(FONT_SIZE_TRIGGER))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text_primary)
                .child(format_cadence(cadence_mins)),
        )
        .child(
            div()
                .text_size(px(FONT_SIZE_ARROW))
                .text_color(theme.text_muted)
                .ml(px(TRIGGER_GAP))
                .child(if dropdown_open { "▲" } else { "▼" }),
        )
        .on_mouse_down(MouseButton::Left, move |_, window, _| {
            let mut s = toggle_state.borrow_mut();
            s.settings_ui.cadence_dropdown_open = !s.settings_ui.cadence_dropdown_open;
            drop(s);
            window.refresh();
        });

    if dropdown_open {
        trigger = trigger.child(render_option_list(state, cadence_mins, theme));
    }

    trigger
}

/// Floating option list shown when the dropdown is open.
fn render_option_list(
    state: &Rc<RefCell<AppState>>,
    cadence_mins: Option<u64>,
    theme: &Theme,
) -> Deferred {
    let bg = theme.bg_base;
    let border = theme.border_strong;
    let state = state.clone();
    let theme = theme.clone();

    deferred(
        div()
            .occlude()
            .absolute()
            .top(px(DROPDOWN_TOP_OFFSET))
            .right(px(0.0))
            .w(px(DROPDOWN_MIN_WIDTH))
            .h(px(DROPDOWN_MAX_HEIGHT))
            .rounded(px(DROPDOWN_RADIUS))
            .bg(bg)
            .border_1()
            .border_color(border)
            .shadow_lg()
            .child(
                uniform_list(
                    "cadence-options-list",
                    REFRESH_OPTIONS.len(),
                    move |range: Range<usize>, _window: &mut Window, _cx: &mut App| {
                        range
                            .map(|i| {
                                let mins = REFRESH_OPTIONS[i];
                                render_option_row(i, mins, cadence_mins, &state, bg, &theme)
                            })
                            .collect::<Vec<_>>()
                    },
                )
                .size_full(),
            ),
    )
    .with_priority(1)
}

/// Single selectable row inside the option list.
fn render_option_row(
    index: usize,
    mins: Option<u64>,
    cadence_mins: Option<u64>,
    state: &Rc<RefCell<AppState>>,
    bg: Hsla,
    theme: &Theme,
) -> Div {
    let is_active = cadence_mins == mins;
    let opt_state = state.clone();

    let mut row = div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .px(px(OPTION_PADDING_X))
        .py(px(OPTION_PADDING_Y))
        .cursor_pointer()
        .bg(if is_active {
            theme.element_selected
        } else {
            bg
        })
        .child(
            div()
                .text_size(px(FONT_SIZE_OPTION))
                .font_weight(if is_active {
                    FontWeight::SEMIBOLD
                } else {
                    FontWeight::MEDIUM
                })
                .text_color(if is_active {
                    theme.element_active
                } else {
                    theme.text_primary
                })
                .child(format_cadence(mins)),
        )
        .when(is_active, |el| {
            el.child(
                div()
                    .text_size(px(FONT_SIZE_CHECK))
                    .font_weight(FontWeight::BOLD)
                    .text_color(theme.element_active)
                    .child("✓"),
            )
        })
        .on_mouse_down(MouseButton::Left, move |_, window, _| {
            let settings = {
                let mut s = opt_state.borrow_mut();
                s.select_cadence(mins);
                s.settings.clone()
            };
            persist_settings(&settings);
            window.refresh();
        });

    if index > 0 {
        row = row.border_t_1().border_color(theme.border_strong);
    }

    row
}
