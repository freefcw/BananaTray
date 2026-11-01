use crate::application::{
    SettingsProviderDetailViewState, SettingsProviderInfoViewState, SettingsProviderStatusKind,
};
use crate::theme::Theme;
use gpui::{div, px, relative, Div, Hsla, ParentElement, Styled};

/// Provider 头部内联状态行：直接消费 selector 预计算的 `info.summary_text` 与状态等级，
/// 替代原先独立占一行的 4 列状态表，在窄宽度下保持单行清晰可读。
pub(super) fn render_status_summary(
    detail: &SettingsProviderDetailViewState,
    theme: &Theme,
) -> Div {
    let dot_color = if !detail.is_enabled || !detail.can_refresh {
        theme.text.muted
    } else {
        status_dot_color(&detail.info, theme)
    };

    let text_color = if detail.is_enabled
        && detail.can_refresh
        && detail.info.status_kind == SettingsProviderStatusKind::Error
    {
        theme.status.error
    } else {
        theme.text.muted
    };

    div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .min_w_0()
        .child(
            div()
                .flex_shrink_0()
                .w(px(7.0))
                .h(px(7.0))
                .rounded_full()
                .bg(dot_color),
        )
        .child(
            div()
                .min_w_0()
                .text_size(px(11.5))
                .line_height(relative(1.3))
                .text_color(text_color)
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(detail.info.summary_text.clone()),
        )
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
