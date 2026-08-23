use super::actions::{render_editable_provider_actions, EditableProviderActions};
use super::DetailActionDispatcher;
use crate::application::SettingsProviderDetailViewState;
use crate::models::{ProviderCapability, SettingsCapability};
use crate::theme::Theme;
use crate::ui::settings_window::SettingsView;
use crate::ui::widgets::{render_detail_section_title, render_svg_icon};
use gpui::{div, hsla, px, Context, Div, ParentElement, Styled, TextAlign};
use rust_i18n::t;

pub(super) fn render_settings_section(
    view: &mut SettingsView,
    detail: &SettingsProviderDetailViewState,
    dispatcher: &DetailActionDispatcher,
    theme: &Theme,
    cx: &mut Context<SettingsView>,
) -> Div {
    let section = settings_section_shell(theme);

    match detail.settings_capability.clone() {
        SettingsCapability::TokenInput(capability) => section.child(
            super::super::token_input_panel::render_token_input_panel(
                &detail.id, capability, view, theme, cx,
            )
            .mt(px(10.0)),
        ),
        SettingsCapability::NewApiEditable { base_url } => section.child(
            render_editable_provider_actions(
                EditableProviderActions::newapi(detail.id.clone(), base_url),
                detail.confirming_delete_newapi,
                dispatcher,
                theme,
            )
            .mt(px(10.0)),
        ),
        SettingsCapability::ScriptEditable { interpreter } => section.child(
            render_editable_provider_actions(
                EditableProviderActions::script_provider(detail.id.clone(), interpreter),
                detail.confirming_delete_script_provider,
                dispatcher,
                theme,
            )
            .mt(px(10.0)),
        ),
        SettingsCapability::None => {
            let placeholder = placeholder_for_provider_capability(detail.provider_capability);
            section.child(render_info_placeholder_card(
                placeholder.title,
                placeholder.description,
            ))
        }
    }
}

fn settings_section_shell(theme: &Theme) -> Div {
    div()
        .flex_col()
        .mt(px(20.0))
        .pb(px(20.0))
        .child(render_detail_section_title(
            &t!("provider.section.settings"),
            theme,
        ))
}

struct PlaceholderText {
    title: String,
    description: String,
}

fn placeholder_for_provider_capability(capability: ProviderCapability) -> PlaceholderText {
    match capability {
        ProviderCapability::Monitorable => PlaceholderText {
            title: t!("provider.settings.auto_title").to_string(),
            description: t!("provider.settings.auto_desc").to_string(),
        },
        ProviderCapability::Informational => PlaceholderText {
            title: t!("provider.informational.title").to_string(),
            description: t!("provider.informational.desc").to_string(),
        },
        ProviderCapability::Placeholder => PlaceholderText {
            title: t!("provider.placeholder.title").to_string(),
            description: t!("provider.placeholder.desc").to_string(),
        },
    }
}

fn render_info_placeholder_card(title: String, description: String) -> Div {
    let muted_color = hsla(0.0, 0.0, 0.45, 0.5);

    div()
        .mt(px(10.0))
        .w_full()
        .flex_col()
        .items_center()
        .justify_center()
        .py(px(36.0))
        .px(px(20.0))
        .rounded(px(12.0))
        .border_1()
        .border_dashed()
        .border_color(hsla(0.0, 0.0, 0.3, 0.3))
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .w_full()
                .child(render_svg_icon(
                    "src/icons/settings.svg",
                    px(32.0),
                    muted_color,
                )),
        )
        .child(
            div()
                .mt(px(16.0))
                .w_full()
                .flex_col()
                .items_center()
                .gap(px(4.0))
                .child(
                    div()
                        .text_size(px(12.5))
                        .text_color(muted_color)
                        .text_align(TextAlign::Center)
                        .child(title),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(muted_color)
                        .text_align(TextAlign::Center)
                        .child(description),
                ),
        )
}
