use super::dropdown::{render_dropdown_panel, render_dropdown_row, render_dropdown_trigger};
use crate::application::{AppAction, SettingChange};
use crate::runtime::AppState;
use crate::theme::Theme;
use gpui::{
    deferred, px, App, Deferred, Div, InteractiveElement, MouseButton, MouseDownEvent,
    ParentElement, Styled, Window,
};
use rust_i18n::t;
use std::cell::RefCell;
use std::rc::Rc;

/// 下拉组件统一宽度
const DROPDOWN_WIDTH: f32 = 140.0;

/// Available refresh cadence options (None = Manual, Some(mins) = Auto)
const OPTIONS: &[Option<u64>] = &[
    None,
    Some(1),
    Some(2),
    Some(3),
    Some(5),
    Some(10),
    Some(15),
    Some(30),
];

fn format_cadence(mins: Option<u64>) -> String {
    match mins {
        None => t!("cadence.manual").to_string(),
        Some(1) => t!("cadence.1_minute").to_string(),
        Some(m) => t!("cadence.n_minutes", n = m).to_string(),
    }
}

/// 内联刷新频率触发按钮 — 风格与设计稿一致（对外开放）
pub(crate) fn render_cadence_trigger(
    state: &Rc<RefCell<AppState>>,
    cadence_mins: Option<u64>,
    theme: &Theme,
) -> Div {
    let dropdown_open = state.borrow().session.settings_ui.cadence_dropdown_open;
    let toggle_state = state.clone();

    let mut trigger = render_dropdown_trigger(
        format_cadence(cadence_mins),
        dropdown_open,
        DROPDOWN_WIDTH,
        false,
        true,
        theme,
    )
    .cursor_pointer()
    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
        crate::bootstrap::dispatch_in_window(
            &toggle_state,
            AppAction::ToggleCadenceDropdown,
            window,
            cx,
        );
    });

    if dropdown_open {
        trigger = trigger.child(render_cadence_options(state, cadence_mins, theme));
    }

    trigger
}

/// 下拉选项列表（内部组件）
fn render_cadence_options(
    state: &Rc<RefCell<AppState>>,
    cadence_mins: Option<u64>,
    theme: &Theme,
) -> Deferred {
    let state = state.clone();

    deferred(
        render_dropdown_panel(DROPDOWN_WIDTH, true, theme)
            .flex()
            .flex_col()
            .gap(px(2.0))
            .children(OPTIONS.iter().map(move |&mins| {
                let is_active = cadence_mins == mins;
                let opt_state = state.clone();
                let label = format_cadence(mins);

                render_dropdown_row(label, is_active, false, theme).on_mouse_down(
                    MouseButton::Left,
                    move |_: &MouseDownEvent, window: &mut Window, cx: &mut App| {
                        crate::bootstrap::dispatch_in_window(
                            &opt_state,
                            AppAction::UpdateSetting(SettingChange::RefreshCadence(mins)),
                            window,
                            cx,
                        );
                    },
                )
            })),
    )
    .with_priority(1)
}
