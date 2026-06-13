use crate::application::{SettingsProviderInfoViewState, SettingsProviderStatusKind};
use crate::theme::Theme;
use crate::ui::widgets::render_info_cell;
use gpui::{div, px, Div, ParentElement, Styled};
use rust_i18n::t;

pub(super) fn render_info_table(info: &SettingsProviderInfoViewState, theme: &Theme) -> Div {
    div()
        .flex_col()
        .gap(px(12.0))
        .mt(px(20.0))
        .child(render_info_row(
            render_info_cell(
                &t!("provider.info.state"),
                &info.state_text,
                theme.text.primary,
                theme,
            ),
            render_info_cell(
                &t!("provider.info.source"),
                &info.source_text,
                theme.text.primary,
                theme,
            ),
        ))
        .child(render_info_row(
            render_info_cell(
                &t!("provider.info.updated"),
                &info.updated_text,
                theme.text.primary,
                theme,
            ),
            render_info_cell(
                &t!("provider.info.status"),
                &info.status_text,
                status_color(info, theme),
                theme,
            ),
        ))
}

fn render_info_row(first: Div, second: Div) -> Div {
    div().flex().gap(px(16.0)).child(first).child(second)
}

fn status_color(info: &SettingsProviderInfoViewState, theme: &Theme) -> gpui::Hsla {
    match info.status_kind {
        SettingsProviderStatusKind::Success => theme.status.success,
        SettingsProviderStatusKind::Error => theme.status.error,
        SettingsProviderStatusKind::Neutral => theme.text.primary,
    }
}
