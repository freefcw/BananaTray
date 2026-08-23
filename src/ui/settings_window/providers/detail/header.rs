use super::{icon_button, provider_id_for_action, DetailActionDispatcher};
use crate::application::AppAction;
use crate::application::SettingsProviderDetailViewState;
use crate::refresh::RefreshReason;
use crate::theme::Theme;
use crate::ui::settings_window::providers::shared;
use gpui::{div, px, Div, FontWeight, InteractiveElement, ParentElement, Styled};
use rust_i18n::t;

pub(super) fn render_header(
    detail: &SettingsProviderDetailViewState,
    dispatcher: &DetailActionDispatcher,
    theme: &Theme,
) -> Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .child(render_header_identity(
            &detail.icon,
            &detail.display_name,
            &detail.subtitle,
            theme,
        ))
        .child(render_header_actions(detail, dispatcher, theme))
}

fn render_header_identity(icon: &str, display_name: &str, subtitle: &str, theme: &Theme) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(14.0))
        .child(crate::ui::widgets::render_provider_icon_boxed(
            icon,
            px(56.0),
            px(32.0),
            theme.text.primary,
            theme.bg.subtle,
        ))
        .child(
            div()
                .flex_col()
                .flex_1()
                .min_w(px(0.0))
                .overflow_hidden()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(px(18.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(theme.text.primary)
                        .child(display_name.to_string()),
                )
                .child(
                    div()
                        .text_size(px(11.5))
                        .text_color(theme.text.muted)
                        .child(subtitle.to_string()),
                ),
        )
}

fn render_header_actions(
    detail: &SettingsProviderDetailViewState,
    dispatcher: &DetailActionDispatcher,
    theme: &Theme,
) -> Div {
    let mut row = div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(render_remove_from_sidebar_button(detail, dispatcher, theme))
        .child(
            crate::ui::widgets::render_toggle_switch(
                detail.is_enabled,
                px(44.0),
                px(24.0),
                px(18.0),
                theme,
            )
            .on_mouse_down(gpui::MouseButton::Left, {
                let provider_id = provider_id_for_action(&detail.id);
                dispatcher
                    .interactive_action(move || AppAction::ToggleProvider(provider_id.clone()))
            }),
        );

    if detail.can_refresh {
        row = row.child(icon_button(None, "src/icons/refresh.svg", 16.0, theme, {
            let provider_id = provider_id_for_action(&detail.id);
            dispatcher.interactive_action(move || AppAction::RefreshProvider {
                id: provider_id.clone(),
                reason: RefreshReason::Manual,
            })
        }));
    }

    row
}

fn render_remove_from_sidebar_button(
    detail: &SettingsProviderDetailViewState,
    dispatcher: &DetailActionDispatcher,
    theme: &Theme,
) -> Div {
    if detail.confirming_remove {
        return shared::render_confirm_cancel_buttons(
            &t!("common.confirm"),
            &t!("common.cancel"),
            {
                let provider_id = provider_id_for_action(&detail.id);
                dispatcher.interactive_action(move || {
                    AppAction::RemoveProviderFromSidebar(provider_id.clone())
                })
            },
            dispatcher.interactive_action(|| AppAction::CancelRemoveProvider),
            theme,
        );
    }

    icon_button(
        Some("remove-from-sidebar"),
        "src/icons/trash.svg",
        14.0,
        theme,
        dispatcher.interactive_cleanup_action(|| AppAction::ConfirmRemoveProvider),
    )
}
