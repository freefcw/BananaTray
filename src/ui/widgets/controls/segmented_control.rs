/// 分段选择器组件
///
/// 圆角容器 + 多个 pill 选项 + 选中高亮的分段控件。
/// 主要用于设置窗口中的 Theme / Language / Log Level 选择。
use crate::theme::Theme;
use gpui::{
    div, px, transparent_black, App, Div, FontWeight, InteractiveElement, MouseButton,
    ParentElement, Styled, Window,
};

/// Div 样式变换函数类型（消除 clippy::type_complexity 警告）
type DivStyleFn = Box<dyn Fn(Div) -> Div>;

/// 分段选择器尺寸风格
#[allow(dead_code)]
pub(crate) enum SegmentedSize {
    /// 全宽等分（用于纵向堆叠布局）
    Full,
    /// 紧凑自适应宽度（用于 Log Level 选择器）
    Compact,
    /// 行内自适应宽度（用于水平行布局，如 Display Tab 的 Theme/Language 选择器）
    Inline,
}

/// 渲染分段选择器
///
/// # 参数
/// - `options` — 选项列表 (显示文字, 值)
/// - `current` — 当前选中的值
/// - `size` — 尺寸风格 (Full / Compact)
/// - `theme` — 主题
/// - `on_select` — 选中回调，接收 (值, &mut Window, &mut App)
///
/// # 使用场景
/// - `settings_window/display_tab.rs` — Theme / Language 分段选择器
/// - `settings_window/debug_tab.rs` — Log Level 分段选择器
pub(crate) fn render_segmented_control<T, F>(
    options: &[(String, T)],
    current: &T,
    size: SegmentedSize,
    theme: &Theme,
    on_select: F,
) -> Div
where
    T: PartialEq + Clone + 'static,
    F: Fn(T, &mut Window, &mut App) + Clone + 'static,
{
    let (text_size_val, container_style, pill_style): (f32, DivStyleFn, DivStyleFn) = match size {
        SegmentedSize::Full => (
            12.0,
            Box::new(|d: Div| d.w_full()),
            Box::new(|d: Div| {
                d.flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .py(px(8.0))
            }),
        ),
        SegmentedSize::Compact => (
            11.0,
            Box::new(|d: Div| d.flex_shrink_0()),
            Box::new(|d: Div| d.px(px(8.0)).py(px(5.0))),
        ),
        SegmentedSize::Inline => (
            12.0,
            Box::new(|d: Div| d.flex_shrink_0()),
            Box::new(|d: Div| {
                d.px(px(14.0))
                    .py(px(7.0))
                    .flex()
                    .items_center()
                    .justify_center()
            }),
        ),
    };

    let mut control = container_style(div())
        .flex()
        .rounded(px(8.0))
        .bg(theme.bg.subtle)
        .border_1()
        .border_color(theme.border.subtle)
        .overflow_hidden();

    for (label, value) in options {
        let is_active = current == value;
        let value_clone = value.clone();
        let on_select_clone = on_select.clone();

        let pill = pill_style(div())
            .rounded(px(7.0))
            .bg(if is_active {
                theme.nav.pill_active_bg
            } else {
                transparent_black()
            })
            .text_size(px(text_size_val))
            // 选中态仅切换颜色与底色，避免字重变化导致宽度和视觉重心抖动。
            .font_weight(FontWeight::MEDIUM)
            .text_color(if is_active {
                theme.element.active
            } else {
                theme.text.secondary
            })
            .cursor_pointer()
            .child(label.clone())
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                on_select_clone(value_clone.clone(), window, cx);
            });

        control = control.child(pill);
    }

    control
}
