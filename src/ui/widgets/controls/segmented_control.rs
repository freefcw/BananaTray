/// 分段选择器组件
///
/// 圆角容器 + 多个 pill 选项 + 选中高亮的分段控件。
/// 用于设置窗口中的 Theme / Language / Log Level 选择，
/// 也复用给 provider 明细页的 History 范围选择。
use crate::theme::Theme;
use gpui::{
    div, px, transparent_black, App, Div, FontWeight, Hsla, InteractiveElement, MouseButton,
    ParentElement, Styled, Window,
};

/// Div 样式变换函数类型（消除 clippy::type_complexity 警告）
type DivStyleFn = Box<dyn Fn(Div) -> Div>;

/// 分段选择器尺寸风格
pub(crate) enum SegmentedSize {
    /// 紧凑自适应宽度（用于 Log Level 选择器）
    Compact,
    /// 行内自适应宽度（用于水平行布局，如 Display Tab 的 Theme/Language 选择器）
    Inline,
    /// History 范围（provider 明细页 24h/7d/30d）：无外边框、紧凑 pill 组
    HistoryRange,
}

/// 单个尺寸的外观配置。颜色始终取自 `Theme`，避免各尺寸在暗/亮主题下自定漂移。
struct SegmentedStyle {
    text_size: f32,
    /// 容器变换（尺寸相关：弹性、内边距等）
    container: DivStyleFn,
    /// pill 变换（尺寸相关：内边距/高度等）
    pill: DivStyleFn,
    /// 是否给容器绘制外边框（HistoryRange 不需要）
    bordered: bool,
    /// pill 圆角
    pill_radius: f32,
    /// 选中/未选中文字色
    active_text: fn(&Theme) -> Hsla,
    inactive_text: fn(&Theme) -> Hsla,
}

fn element_active(theme: &Theme) -> Hsla {
    theme.element.active
}
fn element_secondary(theme: &Theme) -> Hsla {
    theme.text.secondary
}
fn nav_active_text(theme: &Theme) -> Hsla {
    theme.nav.pill_active_text
}
fn nav_inactive_text(theme: &Theme) -> Hsla {
    theme.text.muted
}

fn compact_container(d: Div) -> Div {
    d.flex_shrink_0()
}
fn compact_pill(d: Div) -> Div {
    d.px(px(8.0)).py(px(5.0))
}

fn inline_container(d: Div) -> Div {
    d.flex_shrink_0()
}
fn inline_pill(d: Div) -> Div {
    d.px(px(14.0))
        .py(px(7.0))
        .flex()
        .items_center()
        .justify_center()
}

fn history_container(d: Div) -> Div {
    d.p(px(2.0)).flex().items_center().gap(px(2.0))
}
fn history_pill(d: Div) -> Div {
    d.h(px(22.0)).px(px(8.0)).flex().items_center()
}

fn style_for(size: SegmentedSize) -> SegmentedStyle {
    match size {
        SegmentedSize::Compact => SegmentedStyle {
            text_size: 11.0,
            container: Box::new(compact_container),
            pill: Box::new(compact_pill),
            bordered: true,
            pill_radius: 7.0,
            active_text: element_active,
            inactive_text: element_secondary,
        },
        SegmentedSize::Inline => SegmentedStyle {
            text_size: 12.0,
            container: Box::new(inline_container),
            pill: Box::new(inline_pill),
            bordered: true,
            pill_radius: 7.0,
            active_text: element_active,
            inactive_text: element_secondary,
        },
        SegmentedSize::HistoryRange => SegmentedStyle {
            text_size: 12.0,
            container: Box::new(history_container),
            pill: Box::new(history_pill),
            bordered: false,
            pill_radius: 4.0,
            active_text: nav_active_text,
            inactive_text: nav_inactive_text,
        },
    }
}

/// 渲染分段选择器
///
/// # 参数
/// - `options` — 选项列表 (显示文字, 值)
/// - `current` — 当前选中的值
/// - `size` — 尺寸风格 (Compact / Inline / HistoryRange)
/// - `theme` — 主题
/// - `on_select` — 选中回调，接收 (值, &mut Window, &mut App)
///
/// # 使用场景
/// - `settings_window/display_tab.rs` — Theme / Language 分段选择器
/// - `settings_window/debug_tab.rs` — Log Level 分段选择器
/// - `settings_window/providers/detail/history.rs` — History 范围
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
    let style = style_for(size);

    let mut control = {
        let base = (style.container)(div().flex());
        if style.bordered {
            base.rounded(px(8.0))
                .bg(theme.bg.subtle)
                .border_1()
                .border_color(theme.border.subtle)
                .overflow_hidden()
        } else {
            base.rounded(px(6.0)).bg(theme.bg.subtle)
        }
    };

    for (label, value) in options {
        let is_active = current == value;
        let value_clone = value.clone();
        let on_select_clone = on_select.clone();

        let pill = (style.pill)(div())
            .rounded(px(style.pill_radius))
            .bg(if is_active {
                theme.nav.pill_active_bg
            } else {
                transparent_black()
            })
            .text_size(px(style.text_size))
            // 选中态仅切换颜色与底色，避免字重变化导致宽度变化和视觉重心抖动。
            .font_weight(FontWeight::MEDIUM)
            .text_color(if is_active {
                (style.active_text)(theme)
            } else {
                (style.inactive_text)(theme)
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
