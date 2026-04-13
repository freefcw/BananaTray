/// 图标设置行组件
///
/// 通用三栏布局：彩色圆形图标 + 标题/描述 + 右侧任意控件。
/// 主要用于设置窗口中各类带图标的设置行。
use super::colored_icon::render_colored_icon;
use crate::theme::Theme;
use gpui::{div, px, Div, FontWeight, Hsla, IntoElement, ParentElement, Styled};

/// 渲染通用图标设置行：彩色圆形图标 + 标题/描述 + 右侧任意控件
///
/// # 使用场景
/// - `settings_window/components.rs` — render_icon_switch_row / render_icon_dropdown_row 的基础
/// - `settings_window/debug_tab.rs` — render_log_level_row / render_test_notification_button
pub(crate) fn render_icon_row(
    icon_path: &'static str,
    icon_color: Hsla,
    icon_bg: Hsla,
    title: &str,
    description: &str,
    theme: &Theme,
    trailing: impl IntoElement,
) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(12.0))
        .px(px(14.0))
        .py(px(12.0))
        // 彩色圆形图标
        .child(render_colored_icon(icon_path, icon_color, icon_bg))
        // 标题 + 描述
        .child(
            div()
                .flex_col()
                .flex_1()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(px(14.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.text.primary)
                        .child(title.to_string()),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(theme.text.muted)
                        .child(description.to_string()),
                ),
        )
        // 右侧控件
        .child(trailing)
}
