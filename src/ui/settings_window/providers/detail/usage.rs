use crate::application::SettingsProviderUsageViewState;
use crate::models::QuotaDisplayMode;
use crate::theme::Theme;
use crate::ui::widgets::render_detail_section_title;
use gpui::{div, px, relative, Div, ParentElement, Styled};
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
            usage_section(theme).child(render_usage_message(message, theme))
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
        SettingsProviderUsageViewState::Error { title, message } => usage_section(theme)
            .child(render_error_title(title, theme))
            .child(render_error_detail(message, theme)),
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

fn render_usage_message(message: &str, theme: &Theme) -> Div {
    div()
        .mt(px(8.0))
        .text_size(px(12.0))
        .text_color(theme.text.secondary)
        .child(message.to_string())
}

fn render_error_title(title: &str, theme: &Theme) -> Div {
    div()
        .mt(px(8.0))
        .text_size(px(12.0))
        .text_color(theme.text.muted)
        .child(title.to_string())
}

fn render_error_detail(message: &str, theme: &Theme) -> Div {
    div()
        .px(px(10.0))
        .py(px(8.0))
        .rounded(px(6.0))
        .bg(theme.bg.subtle)
        .child(
            div()
                .text_size(px(11.5))
                .line_height(relative(1.4))
                .text_color(theme.text.secondary)
                .child(message.to_string()),
        )
}
