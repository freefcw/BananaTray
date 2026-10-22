use crate::application::{SettingsProviderInfoViewState, SettingsProviderStatusKind};
use crate::theme::Theme;
use gpui::{div, px, relative, Div, FontWeight, Hsla, ParentElement, Styled};
use rust_i18n::t;

/// Provider 概览：与额度卡同款的填充卡片，内部 2×2 排布，标签在上、值在下。
///
/// 值独占整列宽度，「更新失败 · 数据 5 分钟前」这类长文案不必和标签抢同一行；
/// 实在放不下时以省略号收尾。服务状态前加一个状态色圆点，扫一眼就能看出是否正常。
pub(super) fn render_info_table(info: &SettingsProviderInfoViewState, theme: &Theme) -> Div {
    let primary = theme.text.primary;
    div()
        .flex()
        .flex_col()
        .gap(px(14.0))
        .mt(px(20.0))
        .px(px(16.0))
        .py(px(14.0))
        .rounded(px(12.0))
        .bg(theme.bg.card_inner)
        .border_1()
        .border_color(theme.border.strong)
        .child(render_info_row(
            render_info_field(
                &t!("provider.info.state"),
                &info.state_text,
                primary,
                None,
                theme,
            ),
            render_info_field(
                &t!("provider.info.source"),
                &info.source_text,
                primary,
                None,
                theme,
            ),
        ))
        .child(render_info_row(
            render_info_field(
                &t!("provider.info.updated"),
                &info.updated_text,
                primary,
                None,
                theme,
            ),
            render_info_field(
                &t!("provider.info.status"),
                &info.status_text,
                status_color(info, theme),
                Some(status_dot_color(info, theme)),
                theme,
            ),
        ))
}

fn render_info_row(first: Div, second: Div) -> Div {
    div().flex().gap(px(16.0)).child(first).child(second)
}

fn render_info_field(
    label: &str,
    value: &str,
    value_color: Hsla,
    dot: Option<Hsla>,
    theme: &Theme,
) -> Div {
    let mut value_row = div().flex().items_center().gap(px(6.0)).min_w_0();
    if let Some(dot) = dot {
        value_row = value_row.child(
            div()
                .flex_shrink_0()
                .w(px(7.0))
                .h(px(7.0))
                .rounded_full()
                .bg(dot),
        );
    }
    value_row = value_row.child(
        div()
            .min_w_0()
            .text_size(px(13.5))
            .line_height(relative(1.3))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(value_color)
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .child(value.to_string()),
    );

    div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .child(
            div()
                .text_size(px(11.5))
                .line_height(relative(1.3))
                .text_color(theme.text.muted)
                .child(label.to_string()),
        )
        .child(value_row)
}

fn status_dot_color(info: &SettingsProviderInfoViewState, theme: &Theme) -> Hsla {
    match info.status_kind {
        SettingsProviderStatusKind::Neutral => theme.text.muted,
        _ => status_color(info, theme),
    }
}

fn status_color(info: &SettingsProviderInfoViewState, theme: &Theme) -> Hsla {
    match info.status_kind {
        SettingsProviderStatusKind::Success => theme.status.success,
        SettingsProviderStatusKind::Error => theme.status.error,
        SettingsProviderStatusKind::Neutral => theme.text.primary,
    }
}
