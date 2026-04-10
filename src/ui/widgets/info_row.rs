use crate::platform::system::open_url;
/// 信息行组件
///
/// 键值对行：左标签（灰色）+ 右值（可选链接）。
/// 主要用于设置窗口 About 页和 Provider 详情面板。
use crate::theme::Theme;
use gpui::*;

/// 渲染信息行（左标签 + 右值），支持可选链接
///
/// # 参数
/// - `label` — 左侧标签文字
/// - `value` — 右侧显示值
/// - `url` — 可选链接，点击右侧值时打开
/// - `value_color` — 右侧值的文字颜色
/// - `theme` — 主题
///
/// # 使用场景
/// - `settings_window/about_tab.rs` — Build Version / Developer / License / Website 行
/// - `settings_window/providers/detail.rs` — Provider 信息单元格
pub(crate) fn render_kv_info_row(
    label: &str,
    value: &str,
    url: Option<&str>,
    value_color: Hsla,
    theme: &Theme,
) -> Div {
    let value_str = value.to_string();

    let mut value_el = div()
        .text_size(px(12.5))
        .font_weight(FontWeight::MEDIUM)
        .text_color(value_color);

    if let Some(link) = url {
        let link_owned = link.to_string();
        value_el = value_el
            .flex()
            .items_center()
            .gap(px(4.0))
            .cursor_pointer()
            .child(value_str)
            .child(div().text_size(px(10.0)).text_color(value_color).child("↗"))
            .on_mouse_down(MouseButton::Left, move |_, _, _| {
                open_url(&link_owned);
            });
    } else {
        value_el = value_el.child(value_str);
    }

    div()
        .flex()
        .items_center()
        .justify_between()
        .w_full()
        .py(px(8.0))
        .child(
            div()
                .text_size(px(12.5))
                .text_color(theme.text.muted)
                .child(label.to_string()),
        )
        .child(value_el)
}

/// 渲染信息单元格（标签 + 值），水平排列用于两列布局
///
/// # 使用场景
/// - `settings_window/providers/detail.rs` — Provider 信息表格单元格
pub(crate) fn render_info_cell(label: &str, value: &str, value_color: Hsla, theme: &Theme) -> Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .flex_1()
        .child(
            div()
                .text_size(px(12.5))
                .text_color(theme.text.muted)
                .flex_shrink_0()
                .child(label.to_string()),
        )
        .child(
            div()
                .text_size(px(13.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(value_color)
                .child(value.to_string()),
        )
}
