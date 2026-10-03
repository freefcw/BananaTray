use crate::theme::Theme;
use crate::ui::widgets::{render_stepper, StepperOptions};
use gpui::{
    div, px, App, Div, InteractiveElement, MouseButton, MouseDownEvent, ParentElement, Styled,
    Window,
};
use rust_i18n::t;
use std::rc::Rc;

/// 步进器中间数值区宽度（容纳「每 10%」/「Every 10%」）。
const VALUE_WIDTH: f32 = 84.0;
const MAX_STEP: u8 = 100;
/// 小于该值按 1 步进，之后按 5 步进，兼顾精细调节与快速到达常用档位。
const FINE_STEP_LIMIT: u8 = 10;
const COARSE_STEP: u8 = 5;

/// `+` 按钮的下一个步长：0..10 逐一递增，≥10 对齐到下一个 5 的倍数。
fn increment_step(step: u8) -> u8 {
    let next = if step < FINE_STEP_LIMIT {
        step + 1
    } else {
        (step / COARSE_STEP + 1) * COARSE_STEP
    };
    next.min(MAX_STEP)
}

/// `−` 按钮的下一个步长：>10 对齐到上一个 5 的倍数（不低于 10），≤10 逐一递减到 0（关闭）。
fn decrement_step(step: u8) -> u8 {
    if step > FINE_STEP_LIMIT {
        ((step - 1) / COARSE_STEP * COARSE_STEP).max(FINE_STEP_LIMIT)
    } else {
        step.saturating_sub(1)
    }
}

fn step_label(step: u8) -> String {
    if step == 0 {
        t!("quota_usage.off").to_string()
    } else {
        t!("quota_usage.every", n = step).to_string()
    }
}

/// 用量提醒步长步进器。
///
/// - `selection`：当前显式设置；`None` 表示跟随全局（仅 provider 级有意义）。
/// - `inherited_step`：`Some` 表示处于 provider 级，可跟随全局；全局设置传 `None`。
///
/// 跟随全局时步进器以弱化色显示全局值，点击 `−`/`+` 会以全局值为起点创建覆盖；
/// 已覆盖时右侧提供「恢复跟随全局」按钮，回调参数为 `None`。
pub(super) fn render_quota_usage_stepper(
    selection: Option<u8>,
    inherited_step: Option<u8>,
    theme: &Theme,
    on_change: impl Fn(Option<u8>, &mut Window, &mut App) + 'static,
) -> Div {
    let on_change = Rc::new(on_change);
    let is_inherited = selection.is_none() && inherited_step.is_some();
    let effective = selection.or(inherited_step).unwrap_or(0);

    let stepper = render_stepper(
        StepperOptions {
            label: step_label(effective),
            muted: is_inherited,
            can_decrement: effective > 0,
            can_increment: effective < MAX_STEP,
            value_width: VALUE_WIDTH,
        },
        theme,
        {
            let on_change = on_change.clone();
            move |_, window, cx| on_change(Some(decrement_step(effective)), window, cx)
        },
        {
            let on_change = on_change.clone();
            move |_, window, cx| on_change(Some(increment_step(effective)), window, cx)
        },
    );

    let mut row = div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(8.0))
        .child(stepper);

    if inherited_step.is_some() {
        row = row.child(if is_inherited {
            div()
                .px(px(8.0))
                .py(px(3.0))
                .rounded(px(6.0))
                .bg(theme.bg.subtle)
                .text_size(px(11.0))
                .text_color(theme.text.muted)
                .child(t!("quota_usage.inherited").to_string())
        } else {
            div()
                .px(px(8.0))
                .py(px(3.0))
                .rounded(px(6.0))
                .cursor_pointer()
                .text_size(px(12.0))
                .text_color(theme.text.accent)
                .debug_selector(|| "quota-usage-follow-global".to_string())
                .hover(|style| style.bg(theme.bg.subtle))
                .child(t!("quota_usage.follow_global").to_string())
                .on_mouse_down(
                    MouseButton::Left,
                    move |_: &MouseDownEvent, window: &mut Window, cx: &mut App| {
                        on_change(None, window, cx);
                    },
                )
        });
    }

    row
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn increment_is_fine_below_ten_then_coarse() {
        assert_eq!(increment_step(0), 1);
        assert_eq!(increment_step(9), 10);
        assert_eq!(increment_step(10), 15);
        assert_eq!(increment_step(12), 15);
        assert_eq!(increment_step(95), 100);
        assert_eq!(increment_step(100), 100);
    }

    #[test]
    fn decrement_is_coarse_above_ten_then_fine_to_off() {
        assert_eq!(decrement_step(100), 95);
        assert_eq!(decrement_step(15), 10);
        assert_eq!(decrement_step(12), 10);
        assert_eq!(decrement_step(11), 10);
        assert_eq!(decrement_step(10), 9);
        assert_eq!(decrement_step(1), 0);
        assert_eq!(decrement_step(0), 0);
    }
}
