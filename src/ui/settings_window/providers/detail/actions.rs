use super::DetailActionDispatcher;
use crate::application::AppAction;
use crate::models::ProviderId;
use crate::theme::Theme;
use crate::ui::settings_window::providers::shared;
use crate::ui::widgets::render_svg_icon;
use gpui::{
    div, px, App, Div, FontWeight, Hsla, InteractiveElement, MouseButton, MouseDownEvent,
    ParentElement, Styled, Window,
};
use rust_i18n::t;

pub(super) struct EditableProviderActions {
    edit_label: String,
    delete_label: String,
    confirm_delete_label: String,
    cancel_delete_label: String,
    kind: EditableProviderKind,
    provider_id: ProviderId,
}

#[derive(Clone, Copy)]
enum EditableProviderKind {
    NewApi,
    ScriptProvider,
}

impl EditableProviderActions {
    pub(super) fn newapi(provider_id: ProviderId) -> Self {
        Self {
            edit_label: t!("newapi.edit_button").to_string(),
            delete_label: t!("newapi.delete_button").to_string(),
            confirm_delete_label: t!("newapi.confirm_delete").to_string(),
            cancel_delete_label: t!("newapi.cancel_delete").to_string(),
            kind: EditableProviderKind::NewApi,
            provider_id,
        }
    }

    pub(super) fn script_provider(provider_id: ProviderId) -> Self {
        Self {
            edit_label: t!("script_provider.edit_button").to_string(),
            delete_label: t!("script_provider.delete_button").to_string(),
            confirm_delete_label: t!("script_provider.confirm_delete").to_string(),
            cancel_delete_label: t!("script_provider.cancel_delete").to_string(),
            kind: EditableProviderKind::ScriptProvider,
            provider_id,
        }
    }

    fn edit_action_factory(&self) -> impl Fn() -> AppAction + 'static {
        let kind = self.kind;
        let provider_id = self.provider_id.clone();
        move || match kind {
            EditableProviderKind::NewApi => AppAction::EditNewApi {
                provider_id: provider_id.clone(),
            },
            EditableProviderKind::ScriptProvider => AppAction::EditScriptProvider {
                provider_id: provider_id.clone(),
            },
        }
    }

    fn confirm_delete_action_factory(&self) -> impl Fn() -> AppAction + 'static {
        let kind = self.kind;
        move || match kind {
            EditableProviderKind::NewApi => AppAction::ConfirmDeleteNewApi,
            EditableProviderKind::ScriptProvider => AppAction::ConfirmDeleteScriptProvider,
        }
    }

    fn delete_action_factory(&self) -> impl Fn() -> AppAction + 'static {
        let kind = self.kind;
        let provider_id = self.provider_id.clone();
        move || match kind {
            EditableProviderKind::NewApi => AppAction::DeleteNewApi {
                provider_id: provider_id.clone(),
            },
            EditableProviderKind::ScriptProvider => AppAction::DeleteScriptProvider {
                provider_id: provider_id.clone(),
            },
        }
    }

    fn cancel_delete_action_factory(&self) -> impl Fn() -> AppAction + 'static {
        let kind = self.kind;
        move || match kind {
            EditableProviderKind::NewApi => AppAction::CancelDeleteNewApi,
            EditableProviderKind::ScriptProvider => AppAction::CancelDeleteScriptProvider,
        }
    }
}

pub(super) fn render_editable_provider_actions(
    actions: EditableProviderActions,
    confirming_delete: bool,
    dispatcher: &DetailActionDispatcher,
    theme: &Theme,
) -> Div {
    let row = div()
        .mt(px(10.0))
        .w_full()
        .flex()
        .items_center()
        .justify_center()
        .gap(px(10.0))
        .child(render_action_button(
            &actions.edit_label,
            "src/icons/settings.svg",
            theme.text.accent,
            theme,
            {
                let action = actions.edit_action_factory();
                dispatcher.interactive_cleanup_action(action)
            },
        ));

    if confirming_delete {
        return row.child(render_confirm_cancel_buttons(
            &actions.confirm_delete_label,
            &actions.cancel_delete_label,
            {
                let action = actions.delete_action_factory();
                dispatcher.interactive_cleanup_action(action)
            },
            {
                let action = actions.cancel_delete_action_factory();
                dispatcher.interactive_cleanup_action(action)
            },
            theme,
        ));
    }

    row.child(render_action_button(
        &actions.delete_label,
        "src/icons/trash.svg",
        theme.status.error,
        theme,
        {
            let action = actions.confirm_delete_action_factory();
            dispatcher.interactive_cleanup_action(action)
        },
    ))
}

pub(super) fn render_confirm_cancel_buttons(
    confirm_label: &str,
    cancel_label: &str,
    on_confirm: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    on_cancel: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    theme: &Theme,
) -> Div {
    shared::render_confirm_cancel_buttons(confirm_label, cancel_label, on_confirm, on_cancel, theme)
}

fn render_action_button(
    label: &str,
    icon: &'static str,
    color: Hsla,
    theme: &Theme,
    on_click: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> Div {
    div()
        .flex()
        .items_center()
        .justify_center()
        .gap(px(5.0))
        .px(px(12.0))
        .py(px(6.0))
        .rounded(px(6.0))
        .bg(theme.bg.subtle)
        .border_1()
        .border_color(theme.border.strong)
        .text_size(px(12.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(color)
        .cursor_pointer()
        .hover(|s| s.opacity(0.85))
        .child(render_svg_icon(icon, px(14.0), color))
        .child(label.to_string())
        .on_mouse_down(MouseButton::Left, on_click)
}
