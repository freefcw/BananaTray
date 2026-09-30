use crate::theme::Theme;
use gpui::{div, px, Div, FontWeight, InteractiveElement, ParentElement, Styled};

/// 触发器的固定高度，浮层从触发器底部对齐展开。
const DROPDOWN_TRIGGER_HEIGHT: f32 = 36.0;

/// 下拉触发按钮外壳：尺寸、边框、开合高亮统一；
/// `truncate` 控制标签超长省略，`enabled` 为 false 时标签置灰（禁用态）。
/// 点击行为与光标样式由调用方按需附加，开合状态由调用方持有。
pub(crate) fn render_dropdown_trigger(
    label: String,
    is_open: bool,
    width: f32,
    truncate: bool,
    enabled: bool,
    theme: &Theme,
) -> Div {
    let mut label_div = div()
        .text_size(px(13.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(if enabled {
            theme.text.primary
        } else {
            theme.text.muted
        })
        .child(label);

    if truncate {
        label_div = label_div
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .whitespace_nowrap();
    }

    div()
        .relative()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_between()
        .w(px(width))
        .gap(px(8.0))
        .px(px(12.0))
        .py(px(6.0))
        .rounded(px(6.0))
        .bg(theme.bg.base)
        .border_1()
        .border_color(if is_open {
            theme.element.selected
        } else {
            theme.border.strong
        })
        .child(label_div)
        .child(
            div()
                .text_size(px(10.0))
                .text_color(theme.text.muted)
                .child(if is_open { "▲" } else { "▼" }),
        )
}

/// 下拉选项浮层外壳：绝对定位、阴影、边框统一；`anchor_right` 控制左右锚点。
/// 列表内容（含滚动容器）由调用方填充，并由调用方包 `deferred` + `with_priority`。
pub(crate) fn render_dropdown_panel(width: f32, anchor_right: bool, theme: &Theme) -> Div {
    let panel = div()
        .occlude()
        .absolute()
        .top(px(DROPDOWN_TRIGGER_HEIGHT))
        .w(px(width))
        .p(px(6.0))
        .rounded(px(8.0))
        .bg(theme.bg.subtle)
        .border_1()
        .border_color(theme.border.strong)
        .shadow_lg();

    if anchor_right {
        panel.right(px(0.0))
    } else {
        panel.left(px(0.0))
    }
}

/// 下拉选项行：选中态高亮加 `✓`，未选中用透明边框避免高度跳动并带 hover 背景。
/// `truncate` 控制标签超长省略；点击行为由调用方注入。
pub(crate) fn render_dropdown_row(
    label: String,
    is_active: bool,
    truncate: bool,
    theme: &Theme,
) -> Div {
    let row = div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .px(px(8.0))
        .py(px(6.0))
        .rounded(px(6.0))
        .cursor_pointer();

    let mut label_div = div().text_size(px(13.0)).child(label);
    if truncate {
        label_div = label_div
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .whitespace_nowrap();
    }

    if is_active {
        row.bg(theme.nav.pill_active_bg)
            .border_1()
            .border_color(theme.element.selected)
            .child(
                label_div
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text.primary),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(theme.text.accent)
                    .child("✓"),
            )
    } else {
        row.border_1()
            .border_color(gpui::transparent_black())
            .hover(|s| s.bg(theme.bg.card_inner_hovered))
            .child(
                label_div
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text.secondary),
            )
    }
}
