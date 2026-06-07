use crate::application::{AppEffect, ContextEffect, ScriptProviderEffect};
use crate::models::{
    unique_script_provider_id as unique_script_provider_id_for_name, ProviderId,
    ScriptProviderConfig, ScriptProviderTestResult,
};

use super::super::state::{AppSession, SettingsModalState};

pub(super) fn enter_add_script_provider(session: &mut AppSession, effects: &mut Vec<AppEffect>) {
    session.settings_ui.modal = SettingsModalState::AddingScriptProvider;
    session.settings_ui.script_provider_testing = false;
    session.settings_ui.script_provider_pending_test_request_id = None;
    session.settings_ui.script_provider_test_result = None;
    session.settings_ui.token_editing_provider = None;
    effects.push(ContextEffect::Render.into());
}

pub(super) fn cancel_add_script_provider(session: &mut AppSession, effects: &mut Vec<AppEffect>) {
    if session.settings_ui.modal.is_script_provider_form() {
        session.settings_ui.modal = SettingsModalState::Idle;
    }
    session.settings_ui.script_provider_testing = false;
    session.settings_ui.script_provider_pending_test_request_id = None;
    session.settings_ui.script_provider_test_result = None;
    session.settings_ui.token_editing_provider = None;
    effects.push(ContextEffect::Render.into());
}

pub(super) fn test_script_provider(
    session: &mut AppSession,
    config: ScriptProviderConfig,
    effects: &mut Vec<AppEffect>,
) {
    session.settings_ui.script_provider_test_request_id = session
        .settings_ui
        .script_provider_test_request_id
        .wrapping_add(1);
    let request_id = session.settings_ui.script_provider_test_request_id;
    session.settings_ui.script_provider_testing = true;
    session.settings_ui.script_provider_pending_test_request_id = Some(request_id);
    session.settings_ui.script_provider_test_result = None;
    effects.push(ScriptProviderEffect::TestProvider { request_id, config }.into());
    effects.push(ContextEffect::Render.into());
}

pub(super) fn script_provider_test_finished(
    session: &mut AppSession,
    request_id: u64,
    result: ScriptProviderTestResult,
    effects: &mut Vec<AppEffect>,
) {
    if session.settings_ui.script_provider_pending_test_request_id != Some(request_id) {
        return;
    }
    session.settings_ui.script_provider_testing = false;
    session.settings_ui.script_provider_pending_test_request_id = None;
    session.settings_ui.script_provider_test_result = Some(result);
    effects.push(ContextEffect::Render.into());
}

pub(super) fn submit_script_provider(
    session: &mut AppSession,
    mut config: ScriptProviderConfig,
    effects: &mut Vec<AppEffect>,
) {
    let is_editing = session
        .settings_ui
        .modal
        .script_provider_edit_data()
        .is_some();
    let original_yaml_filename = session
        .settings_ui
        .modal
        .script_provider_edit_data()
        .map(|data| data.original_yaml_filename.clone());
    let original_script_filename = session
        .settings_ui
        .modal
        .script_provider_edit_data()
        .map(|data| data.original_script_filename.clone());

    if !is_editing {
        config.provider_id = unique_script_provider_id_for_session(session, &config.display_name);
    }
    let new_id = ProviderId::Custom(config.provider_id.clone());
    if !session
        .settings
        .provider
        .enabled_providers
        .contains_key(&new_id.id_key())
    {
        session.settings.provider.set_enabled(&new_id, true);
    }
    if !session
        .settings
        .provider
        .sidebar_providers
        .contains(&new_id.id_key())
    {
        session.settings.provider.add_to_sidebar(&new_id);
    }
    session.settings_ui.selected_provider = new_id;

    effects.push(
        ScriptProviderEffect::SaveProvider {
            config,
            original_yaml_filename,
            original_script_filename,
            is_editing,
        }
        .into(),
    );

    session.settings_ui.modal = SettingsModalState::Idle;
    session.settings_ui.script_provider_testing = false;
    session.settings_ui.script_provider_pending_test_request_id = None;
    session.settings_ui.script_provider_test_result = None;
    session.settings_ui.token_editing_provider = None;
    effects.push(ContextEffect::Render.into());
}

fn unique_script_provider_id_for_session(session: &AppSession, display_name: &str) -> String {
    unique_script_provider_id_for_name(display_name, |id| {
        session.is_script_provider_id_occupied(id)
    })
}

pub(super) fn edit_script_provider(
    session: &mut AppSession,
    provider_id: ProviderId,
    effects: &mut Vec<AppEffect>,
) {
    session.settings_ui.script_provider_testing = false;
    session.settings_ui.script_provider_pending_test_request_id = None;
    session.settings_ui.script_provider_test_result = None;
    session.settings_ui.token_editing_provider = None;
    effects.push(ScriptProviderEffect::LoadConfig { provider_id }.into());
    effects.push(ContextEffect::Render.into());
}

pub(super) fn delete_script_provider(
    session: &mut AppSession,
    provider_id: ProviderId,
    effects: &mut Vec<AppEffect>,
) {
    if session
        .settings_ui
        .modal
        .is_confirming_delete_script_provider()
    {
        session.settings_ui.modal = SettingsModalState::Idle;
    }
    session.settings_ui.token_editing_provider = None;
    effects.push(ContextEffect::Render.into());
    effects.push(ScriptProviderEffect::DeleteProvider { provider_id }.into());
}

pub(super) fn confirm_delete_script_provider(
    session: &mut AppSession,
    effects: &mut Vec<AppEffect>,
) {
    session.settings_ui.modal = SettingsModalState::ConfirmingDeleteScriptProvider;
    session.settings_ui.token_editing_provider = None;
    effects.push(ContextEffect::Render.into());
}

pub(super) fn cancel_delete_script_provider(
    session: &mut AppSession,
    effects: &mut Vec<AppEffect>,
) {
    if session
        .settings_ui
        .modal
        .is_confirming_delete_script_provider()
    {
        session.settings_ui.modal = SettingsModalState::Idle;
    }
    session.settings_ui.token_editing_provider = None;
    effects.push(ContextEffect::Render.into());
}
