use crate::theme::Theme;
use gpui::{
    div, px, App, Div, FontWeight, InteractiveElement, MouseButton, MouseDownEvent, ParentElement,
    Styled, Window,
};

/// 步进器整体高度，与下拉触发器（36px）视觉对齐。
const STEPPER_HEIGHT: f32 = 32.0;
const STEPPER_BUTTON_WIDTH: f32 = 32.0;

/// 步进器的视觉参数；数值格式与步长策略由调用方决定。
pub(crate) struct StepperOptions {
    /// 中间显示的数值文本（如 `5%` / `关闭`）
    pub label: String,
    /// 数值是否以弱化色显示（例如继承自全局而非显式设置）
    pub muted: bool,
    pub can_decrement: bool,
    pub can_increment: bool,
    /// 中间数值区域的宽度
    pub value_width: f32,
}

/// 紧凑步进器：`[−] value [+]`，按钮在边界值时自动禁用。
pub(crate) fn render_stepper(
    options: StepperOptions,
    theme: &Theme,
    on_decrement: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    on_increment: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> Div {
    let StepperOptions {
        label,
        muted,
        can_decrement,
        can_increment,
        value_width,
    } = options;

    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(STEPPER_HEIGHT))
        .rounded(px(8.0))
        .bg(theme.bg.base)
        .border_1()
        .border_color(theme.border.strong)
        .overflow_hidden()
        .child(
            stepper_button("−", can_decrement, theme, on_decrement)
                .debug_selector(|| "stepper-decrement".to_string()),
        )
        .child(
            div()
                .h_full()
                .w(px(value_width))
                .flex()
                .items_center()
                .justify_center()
                .border_l_1()
                .border_r_1()
                .border_color(theme.border.subtle)
                .text_size(px(13.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(if muted {
                    theme.text.muted
                } else {
                    theme.text.primary
                })
                .child(label),
        )
        .child(
            stepper_button("+", can_increment, theme, on_increment)
                .debug_selector(|| "stepper-increment".to_string()),
        )
}

fn stepper_button(
    glyph: &'static str,
    enabled: bool,
    theme: &Theme,
    on_click: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> Div {
    let button = div()
        .h_full()
        .w(px(STEPPER_BUTTON_WIDTH))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(15.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(if enabled {
            theme.text.primary
        } else {
            theme.text.muted.opacity(0.4)
        })
        .child(glyph);

    if !enabled {
        return button;
    }

    let hover_bg = theme.bg.subtle;
    let accent = theme.text.accent;
    button
        .cursor_pointer()
        .hover(move |style| style.bg(hover_bg).text_color(accent))
        .on_mouse_down(MouseButton::Left, on_click)
}
