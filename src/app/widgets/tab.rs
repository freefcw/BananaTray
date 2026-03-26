use crate::theme::Theme;
use gpui::*;

/// 设置窗口顶部 tab 栏项（icon + label + 下划线指示器）
pub(crate) fn render_icon_tab(
    icon_path: &'static str,
    label: &str,
    active: bool,
    _theme: &Theme,
) -> Div {
    let active_color: Hsla = rgb(0x007aff).into();
    let inactive_color: Hsla = rgb(0x8e8e93).into();
    let active_bg: Hsla = rgb(0xe3eefa).into();

    div()
        .flex()
        .flex_col()
        .items_center()
        .gap(px(2.0))
        .px(px(14.0))
        .pt(px(4.0))
        .pb(px(8.0))
        .cursor_pointer()
        .border_b_2()
        .border_color(if active {
            active_color
        } else {
            transparent_black()
        })
        .child(
            div()
                .w(px(30.0))
                .h(px(30.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(8.0))
                .bg(if active {
                    active_bg
                } else {
                    transparent_black()
                })
                .child(svg().path(icon_path).size(px(17.0)).text_color(if active {
                    active_color
                } else {
                    inactive_color
                })),
        )
        .child(
            div()
                .text_size(px(11.5))
                .font_weight(if active {
                    FontWeight::SEMIBOLD
                } else {
                    FontWeight::MEDIUM
                })
                .text_color(if active { active_color } else { inactive_color })
                .child(label.to_string()),
        )
}
