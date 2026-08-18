use crate::application::SettingsProviderUsageViewState;
use crate::models::QuotaDisplayMode;
use crate::theme::Theme;
use crate::ui::widgets::{
    render_detail_empty_card, render_detail_error_card, render_detail_section_title,
};
use gpui::{div, px, Div, ParentElement, Styled};
use rust_i18n::t;

pub(super) fn render_usage_section(
    usage: &SettingsProviderUsageViewState,
    theme: &Theme,
    display_mode: QuotaDisplayMode,
) -> Div {
    match usage {
        SettingsProviderUsageViewState::Disabled { message }
        | SettingsProviderUsageViewState::Empty { message }
        | SettingsProviderUsageViewState::Missing { message } => {
            usage_section(theme).child(render_detail_empty_card(message, theme))
        }
        SettingsProviderUsageViewState::Quotas { quotas } => {
            let mut section = usage_section(theme);
            for quota in quotas {
                section = section.child(div().mt(px(10.0)).child(
                    crate::ui::widgets::render_quota_bar(quota, theme, 0, display_mode),
                ));
            }
            section
        }
        SettingsProviderUsageViewState::Error { title, message } => {
            usage_section(theme).child(render_detail_error_card(title, message, theme))
        }
    }
}

fn usage_section(theme: &Theme) -> Div {
    div()
        .flex_col()
        .mt(px(20.0))
        .child(render_detail_section_title(
            &t!("provider.section.usage"),
            theme,
        ))
}
