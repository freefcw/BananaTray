use crate::application::{AppAction, SettingChange};
use crate::runtime;
use crate::theme::Theme;
use crate::ui::AppState;
use gpui::{
    deferred, div, px, App, Deferred, Div, FontWeight, InteractiveElement, MouseButton,
    MouseDownEvent, ParentElement, Styled, Window,
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

    let mut trigger = div()
        .relative()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_between()
        .w(px(DROPDOWN_WIDTH))
        .gap(px(8.0))
        .px(px(12.0))
        .py(px(6.0))
        .rounded(px(6.0))
        .bg(theme.bg.base)
        .border_1()
        .border_color(if dropdown_open {
            theme.element.selected
        } else {
            theme.border.strong
        })
        .cursor_pointer()
        .child(
            div()
                .text_size(px(13.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text.primary)
                .child(format_cadence(cadence_mins)),
        )
        .child(
            div()
                .text_size(px(10.0))
                .text_color(theme.text.muted)
                .child(if dropdown_open { "▲" } else { "▼" }),
        )
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            runtime::dispatch_in_window(
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
    let theme = theme.clone();

    deferred(
        div()
            .occlude()
            .absolute()
            .top(px(36.0)) // Slight offset from the trigger button
            .right(px(0.0))
            .w(px(DROPDOWN_WIDTH)) // 与触发按钮保持一致的宽度
            .p(px(6.0)) // Inner padding for the popup shell
            .rounded(px(8.0))
            .bg(theme.bg.subtle)
            .border_1()
            .border_color(theme.border.strong)
            .shadow_lg()
            .flex()
            .flex_col()
            .gap(px(2.0)) // Gap between items
            .children(OPTIONS.iter().map(move |&mins| {
                let is_active = cadence_mins == mins;
                let opt_state = state.clone();
                let th = theme.clone();
                let label = format_cadence(mins);

                let mut row = div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(8.0))
                    .py(px(6.0))
                    .rounded(px(6.0))
                    .cursor_pointer();

                if is_active {
                    row = row
                        .bg(th.nav.pill_active_bg) // Subtle active background
                        .border_1()
                        .border_color(th.element.selected) // Distinct outline
                        .child(
                            div()
                                .text_size(px(13.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(th.text.primary)
                                .child(label),
                        )
                        .child(
                            div()
                                .text_size(px(11.0))
                                .font_weight(FontWeight::BOLD)
                                .text_color(th.text.accent)
                                .child("✓"),
                        );
                } else {
                    row = row
                        .border_1()
                        .border_color(gpui::transparent_black()) // Transparent border to prevent height jumping
                        .hover(|s| s.bg(th.bg.card_inner_hovered)) // Hover effect
                        .child(
                            div()
                                .text_size(px(13.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(th.text.secondary)
                                .child(label),
                        );
                }

                row.on_mouse_down(
                    MouseButton::Left,
                    move |_: &MouseDownEvent, window: &mut Window, cx: &mut App| {
                        runtime::dispatch_in_window(
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
