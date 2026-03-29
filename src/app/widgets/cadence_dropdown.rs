use crate::app::{persist_settings, AppState};
use crate::theme::Theme;
use gpui::prelude::FluentBuilder;
use gpui::*;
use std::cell::RefCell;
use std::rc::Rc;

/// Available refresh cadence options (in minutes)
const REFRESH_OPTIONS: &[u64] = &[1, 2, 3, 5, 10, 15, 30];

/// Format a cadence option for display
fn format_cadence(mins: u64) -> String {
    if mins == 1 {
        "1 minute".to_string()
    } else {
        format!("{} minutes", mins)
    }
}

/// Render a cadence dropdown row (collapsed trigger + optional expanded option list).
///
/// Returns a `Div` that can be placed inside a card.
pub(crate) fn render_cadence_dropdown(
    state: &Rc<RefCell<AppState>>,
    cadence_mins: u64,
    theme: &Theme,
) -> Div {
    let dropdown_open = state.borrow().settings_ui.cadence_dropdown_open;
    let toggle_state = state.clone();

    let mut cadence_row = div().flex_col().child(
        div()
            .flex()
            .items_center()
            .justify_between()
            .px(px(14.0))
            .py(px(10.0))
            .child(
                div()
                    .flex_col()
                    .gap(px(2.0))
                    .flex_1()
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::MEDIUM)
                            .child("Refresh cadence"),
                    )
                    .child(
                        div()
                            .text_size(px(12.5))
                            .line_height(relative(1.4))
                            .text_color(theme.text_secondary)
                            .child("How often BananaTray polls providers in the background."),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(4.0))
                    .ml(px(12.0))
                    .px(px(10.0))
                    .py(px(5.0))
                    .rounded(px(6.0))
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
                            .text_size(px(12.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text_primary)
                            .child(format_cadence(cadence_mins)),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme.text_muted)
                            .ml(px(4.0))
                            .child(if dropdown_open { "▲" } else { "▼" }),
                    )
                    .on_mouse_down(MouseButton::Left, move |_, window, _| {
                        let mut s = toggle_state.borrow_mut();
                        s.settings_ui.cadence_dropdown_open = !s.settings_ui.cadence_dropdown_open;
                        drop(s);
                        window.refresh();
                    }),
            ),
    );

    if dropdown_open {
        let option_state = state.clone();
        cadence_row = cadence_row.child(
            div()
                .flex_col()
                .mx(px(14.0))
                .mb(px(8.0))
                .rounded(px(8.0))
                .bg(theme.bg_subtle)
                .border_1()
                .border_color(theme.border_strong)
                .overflow_hidden()
                .children(REFRESH_OPTIONS.iter().enumerate().map(|(i, &mins)| {
                    let is_active = cadence_mins == mins;
                    let opt_state = option_state.clone();
                    let mut row = div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px(px(12.0))
                        .py(px(7.0))
                        .cursor_pointer()
                        .bg(if is_active {
                            theme.element_selected
                        } else {
                            transparent_black()
                        })
                        .child(
                            div()
                                .text_size(px(12.5))
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
                                    .text_size(px(11.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(theme.element_active)
                                    .child("✓"),
                            )
                        })
                        .on_mouse_down(MouseButton::Left, move |_, window, _| {
                            let settings = {
                                let mut s = opt_state.borrow_mut();
                                s.settings.refresh_interval_mins = mins;
                                s.settings_ui.cadence_dropdown_open = false;
                                s.sync_config_to_coordinator();
                                s.settings.clone()
                            };
                            persist_settings(&settings);
                            window.refresh();
                        });
                    // separator between items (not before first)
                    if i > 0 {
                        row = row.border_t_1().border_color(rgb(0xe0e0e4));
                    }
                    row
                })),
        );
    }

    cadence_row
}
