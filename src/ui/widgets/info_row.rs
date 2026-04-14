use crate::platform::system::{open_path_in_finder, open_url};
/// 信息行组件
///
/// 键值对行：左标签（灰色）+ 右值（可选链接）。
/// 主要用于设置窗口 About 页和 Provider 详情面板。
use crate::theme::Theme;
use crate::ui::widgets::with_tooltip;
use gpui::{
    div, px, Div, ElementId, FontWeight, Hsla, InteractiveElement, MouseButton, ParentElement,
    Stateful, Styled,
};
use rust_i18n::t;

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
        .gap(px(8.0))
        .w_full()
        .py(px(8.0))
        .child(
            div()
                .text_size(px(12.5))
                .text_color(theme.text.muted)
                .flex_shrink_0()
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
        .gap(px(6.0))
        .flex_1()
        .min_w(px(0.0))
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
                .overflow_hidden()
                .whitespace_nowrap()
                .child(value.to_string()),
        )
}

/// 可点击的路径信息单元格 — 点击在文件管理器中打开所在目录，悬浮显示 tooltip
///
/// # 使用场景
/// - `settings_window/debug_tab.rs` — 配置文件路径 / 日志路径行
pub(crate) fn render_path_info_cell(
    id: impl Into<ElementId>,
    label: &str,
    path: &str,
    theme: &Theme,
) -> Stateful<Div> {
    let path_buf = std::path::PathBuf::from(path);

    let row = div()
        .cursor_pointer()
        .hover(|s| s.opacity(0.75))
        .child(render_info_cell(label, path, theme.text.accent, theme))
        .on_mouse_down(MouseButton::Left, move |_, _, _| {
            open_path_in_finder(&path_buf);
        });

    with_tooltip(id, &t!("debug.env.open_in_finder"), theme, row)
}
