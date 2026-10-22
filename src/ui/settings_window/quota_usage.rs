use super::providers::shared::render_settings_card;
use crate::theme::Theme;
use crate::ui::widgets::{render_stepper, render_svg_icon, StepperOptions};
use gpui::{
    div, px, relative, App, Div, FontWeight, InteractiveElement, MouseButton, MouseDownEvent,
    ParentElement, Styled, Window,
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
/// 跟随全局时以弱化色显示全局值，点击 `−`/`+` 会以全局值为起点创建覆盖。
/// 「跟随全局」状态标签和恢复按钮由 [`render_provider_quota_usage_card`] 放在卡片标题旁。
pub(super) fn render_quota_usage_stepper(
    selection: Option<u8>,
    inherited_step: Option<u8>,
    theme: &Theme,
    on_change: impl Fn(Option<u8>, &mut Window, &mut App) + 'static,
) -> Div {
    let on_change = Rc::new(on_change);
    let is_inherited = selection.is_none() && inherited_step.is_some();
    let effective = selection.or(inherited_step).unwrap_or(0);

    render_stepper(
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
        move |_, window, cx| on_change(Some(increment_step(effective)), window, cx),
    )
}

/// Provider 详情页的「用量提醒」卡片：上层图标 + 标题 / 状态标签 / 说明，
/// 下层（细分隔线下方）左侧「提醒步长」，右侧步进器。
///
/// 状态标签与下方「额度状态与提醒阈值」卡片的「跟随全局 / 自定义」一致。
/// 已覆盖时步进器左侧出现「恢复跟随全局」文字按钮（回调参数为 `None`），
/// 两种状态下卡片高度一致，切换时不跳动。
pub(super) fn render_provider_quota_usage_card(
    selection: Option<u8>,
    global_step: u8,
    theme: &Theme,
    on_change: impl Fn(Option<u8>, &mut Window, &mut App) + 'static,
) -> Div {
    let on_change = Rc::new(on_change);
    let is_custom = selection.is_some();
    let accent = theme.text.accent;
    let (tag_label, tag_fg, tag_bg) = if is_custom {
        (
            t!("quota_usage.custom").to_string(),
            accent,
            accent.opacity(0.14),
        )
    } else {
        (
            t!("quota_usage.inherited").to_string(),
            theme.text.secondary,
            theme.bg.subtle,
        )
    };

    let icon = div()
        .flex_shrink_0()
        .w(px(36.0))
        .h(px(36.0))
        .rounded(px(10.0))
        .flex()
        .items_center()
        .justify_center()
        .bg(accent.opacity(0.14))
        .child(render_svg_icon("src/icons/usage.svg", px(18.0), accent));

    let title = div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(
            div()
                .text_size(px(14.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.text.primary)
                .whitespace_nowrap()
                .child(t!("provider.quota_usage.title").to_string()),
        )
        .child(
            div()
                .px(px(7.0))
                .py(px(1.0))
                .rounded_full()
                .bg(tag_bg)
                .text_size(px(10.5))
                .font_weight(FontWeight::MEDIUM)
                .text_color(tag_fg)
                .whitespace_nowrap()
                .child(tag_label),
        );

    let text = div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(3.0))
        .child(title)
        .child(
            div()
                .text_size(px(12.0))
                .line_height(relative(1.4))
                .text_color(theme.text.muted)
                .child(t!("provider.quota_usage.desc").to_string()),
        );

    let mut control = div().flex_shrink_0().flex().items_center().gap(px(6.0));
    if is_custom {
        // 用文字而不是图标：同一页顶部的刷新按钮已经用了刷新图标，图标会让人以为是「刷新数据」。
        let hover_bg = theme.bg.card_inner_hovered;
        let on_change = on_change.clone();
        control = control.child(
            div()
                .px(px(8.0))
                .py(px(4.0))
                .rounded(px(6.0))
                .cursor_pointer()
                .text_size(px(12.0))
                .text_color(theme.text.accent)
                .whitespace_nowrap()
                .debug_selector(|| "quota-usage-follow-global".to_string())
                .hover(move |style| style.bg(hover_bg))
                .child(t!("quota_usage.follow_global").to_string())
                .on_mouse_down(
                    MouseButton::Left,
                    move |_: &MouseDownEvent, window: &mut Window, cx: &mut App| {
                        on_change(None, window, cx);
                    },
                ),
        );
    }
    control = control.child(render_quota_usage_stepper(
        selection,
        Some(global_step),
        theme,
        move |step, window, cx| on_change(step, window, cx),
    ));

    let header = div()
        .w_full()
        .flex()
        .items_center()
        .gap(px(12.0))
        .child(icon)
        .child(text);

    let step_row = div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .mt(px(14.0))
        .pt(px(14.0))
        .border_t_1()
        .border_color(theme.border.subtle)
        .child(
            div()
                .text_size(px(12.5))
                .text_color(theme.text.secondary)
                .whitespace_nowrap()
                .child(t!("quota_usage.step_label").to_string()),
        )
        .child(control);

    render_settings_card(theme).child(header).child(step_row)
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
