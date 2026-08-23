use super::DetailActionDispatcher;
use crate::application::AppAction;
use crate::models::ProviderId;
use crate::theme::Theme;
use crate::ui::settings_window::providers::shared;
use crate::ui::widgets::{render_action_button, render_kv_info_row, ButtonSize, ButtonVariant};
use gpui::{div, px, Div, ParentElement, Styled};
use rust_i18n::t;

pub(super) struct EditableProviderActions {
    card_title: String,
    config_row: Option<ConfigSummaryRow>,
    edit_label: String,
    delete_label: String,
    confirm_delete_label: String,
    cancel_delete_label: String,
    kind: EditableProviderKind,
    provider_id: ProviderId,
}

/// 设置卡片里的当前配置摘要行 —— 让用户看清这张卡管的是哪份配置，而不是只有两个按钮。
struct ConfigSummaryRow {
    label: String,
    value: String,
    /// 值本身是可打开的地址时填入，行内会带跳转标记
    url: Option<String>,
}

#[derive(Clone, Copy)]
enum EditableProviderKind {
    NewApi,
    ScriptProvider,
}

impl EditableProviderActions {
    pub(super) fn newapi(provider_id: ProviderId, base_url: String) -> Self {
        Self {
            card_title: t!("newapi.settings_title").to_string(),
            config_row: (!base_url.is_empty()).then(|| ConfigSummaryRow {
                label: t!("newapi.field.url").to_string(),
                value: base_url.clone(),
                url: Some(base_url),
            }),
            edit_label: t!("newapi.edit_button").to_string(),
            delete_label: t!("newapi.delete_button").to_string(),
            confirm_delete_label: t!("newapi.confirm_delete").to_string(),
            cancel_delete_label: t!("newapi.cancel_delete").to_string(),
            kind: EditableProviderKind::NewApi,
            provider_id,
        }
    }

    pub(super) fn script_provider(provider_id: ProviderId, interpreter: String) -> Self {
        Self {
            card_title: t!("script_provider.settings_title").to_string(),
            config_row: (!interpreter.is_empty()).then(|| ConfigSummaryRow {
                label: t!("script_provider.field.interpreter").to_string(),
                value: interpreter,
                url: None,
            }),
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
    let button_row = if confirming_delete {
        action_row()
            .child(render_action_button(
                &actions.confirm_delete_label,
                Some(("src/icons/trash.svg", theme.status.error)),
                ButtonVariant::Danger,
                ButtonSize::Panel,
                theme,
                {
                    let action = actions.delete_action_factory();
                    dispatcher.interactive_cleanup_action(action)
                },
            ))
            .child(render_action_button(
                &actions.cancel_delete_label,
                Some(("src/icons/close.svg", theme.text.primary)),
                ButtonVariant::Subtle,
                ButtonSize::Panel,
                theme,
                {
                    let action = actions.cancel_delete_action_factory();
                    dispatcher.interactive_cleanup_action(action)
                },
            ))
    } else {
        action_row()
            .child(render_action_button(
                &actions.edit_label,
                Some(("src/icons/settings.svg", theme.text.primary)),
                ButtonVariant::Subtle,
                ButtonSize::Panel,
                theme,
                {
                    let action = actions.edit_action_factory();
                    dispatcher.interactive_cleanup_action(action)
                },
            ))
            .child(render_action_button(
                &actions.delete_label,
                Some(("src/icons/trash.svg", theme.text.primary)),
                ButtonVariant::Subtle,
                ButtonSize::Panel,
                theme,
                {
                    let action = actions.confirm_delete_action_factory();
                    dispatcher.interactive_cleanup_action(action)
                },
            ))
    };

    let mut card = shared::render_settings_card(theme).child(shared::render_settings_card_title(
        &actions.card_title,
        theme,
    ));

    if let Some(row) = &actions.config_row {
        card = card.child(render_kv_info_row(
            &row.label,
            &row.value,
            row.url.as_deref(),
            theme,
        ));
    }

    card.child(button_row)
}

fn action_row() -> Div {
    div().w_full().flex().items_center().gap(px(10.0))
}
