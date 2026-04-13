/// 彩色圆形图标组件
///
/// 设计稿核心元素：圆形背景 + 居中 SVG 图标。
/// 主要用于设置窗口的各类设置行（General / Display / Debug tab）。
use super::render_svg_icon;
use gpui::{div, px, Div, Hsla, ParentElement, Styled};

/// 标准尺寸（36×36 圆形 + 18px 图标），用于设置行
pub(crate) fn render_colored_icon(icon_path: &'static str, icon_color: Hsla, icon_bg: Hsla) -> Div {
    render_colored_icon_sized(icon_path, icon_color, icon_bg, 36.0, 18.0)
}

/// 自定义尺寸的彩色圆形图标
///
/// # 使用场景
/// - `settings_window/components.rs` — icon_switch_row / icon_dropdown_row (36px)
/// - `settings_window/debug_tab.rs` — log_level_row / test_notification_button (36px)
/// - `settings_window/debug_tab.rs` — environment_card 头部图标 (28px)
pub(crate) fn render_colored_icon_sized(
    icon_path: &'static str,
    icon_color: Hsla,
    icon_bg: Hsla,
    container_size: f32,
    icon_size: f32,
) -> Div {
    div()
        .w(px(container_size))
        .h(px(container_size))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(icon_bg)
        .flex_shrink_0()
        .child(render_svg_icon(icon_path, px(icon_size), icon_color))
}
