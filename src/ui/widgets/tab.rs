use crate::theme::Theme;
use gpui::*;

/// 设置窗口顶部 tab 栏项（icon + label + 下划线指示器）
#[allow(dead_code)]
pub(crate) fn render_icon_tab(
    icon_path: &'static str,
    label: &str,
    active: bool,
    theme: &Theme,
) -> Div {
    div()
        .flex()
        .flex_col()
        .items_center()
        .gap(px(1.0))
        .px(px(10.0))
        .pt(px(2.0))
        .pb(px(6.0))
        .cursor_pointer()
        .border_b_2()
        .border_color(if active {
            theme.text.accent
        } else {
            transparent_black()
        })
        .child(
            div()
                .w(px(22.0))
                .h(px(22.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(5.0))
                .bg(if active {
                    theme.bg.card
                } else {
                    transparent_black()
                })
                .child(svg().path(icon_path).size(px(15.0)).text_color(if active {
                    theme.text.accent
                } else {
                    theme.text.muted
                })),
        )
        .child(
            div()
                .text_size(px(11.5))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(if active {
                    theme.text.accent
                } else {
                    theme.text.muted
                })
                .child(label.to_string()),
        )
}
