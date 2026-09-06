use crate::application::{
    script_provider_ops, AppEffect, ContextEffect, RefreshEffect, ScriptProviderEffect,
    SettingsEffect,
};
use crate::models::{
    unique_script_provider_id as unique_script_provider_id_for_name,
    CustomProviderLifecycleFailure, ProviderId, ScriptProviderConfig, ScriptProviderDeleteSuccess,
    ScriptProviderEditData, ScriptProviderSaveSuccess, ScriptProviderTestResult,
};
use crate::refresh::RefreshRequest;

use super::super::state::{AppSession, SettingsModalState};

pub(super) struct ScriptProviderSaveCompletion {
    pub(super) request_id: u64,
    pub(super) config: ScriptProviderConfig,
    pub(super) yaml_filename: String,
    pub(super) script_filename: String,
    pub(super) is_editing: bool,
    pub(super) result: Result<ScriptProviderSaveSuccess, CustomProviderLifecycleFailure>,
}

pub(super) fn enter_add_script_provider(session: &mut AppSession, effects: &mut Vec<AppEffect>) {
    session.settings_ui.modal = SettingsModalState::AddingScriptProvider;
    session.settings_ui.clear_script_provider_transient_state();
    session.settings_ui.token_editing_provider = None;
    effects.push(ContextEffect::Render.into());
}

pub(super) fn cancel_add_script_provider(session: &mut AppSession, effects: &mut Vec<AppEffect>) {
    if session.settings_ui.modal.is_script_provider_form() {
        session.settings_ui.modal = SettingsModalState::Idle;
    }
    session.settings_ui.clear_script_provider_transient_state();
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
    let (is_editing, original_yaml_filename, original_script_filename) = session
        .settings_ui
        .modal
        .script_provider_edit_data()
        .map(|data| {
            (
                true,
                Some(data.original_yaml_filename.clone()),
                Some(data.original_script_filename.clone()),
            )
        })
        .unwrap_or((false, None, None));

    if !is_editing {
        config.provider_id = unique_script_provider_id_for_session(session, &config.display_name);
    }
    let new_id = ProviderId::Custom(config.provider_id.clone());
    if !session.settings.provider.has_layout_item(&new_id) {
        session.settings.provider.set_enabled(&new_id, true);
    }
    session.settings.provider.add_to_sidebar(&new_id);
    session.settings_ui.selected_provider = new_id;

    let request_id = session.settings_ui.begin_custom_provider_save();
    effects.push(
        ScriptProviderEffect::SaveProvider {
            request_id,
            config,
            original_yaml_filename,
            original_script_filename,
            is_editing,
        }
        .into(),
    );

    session.settings_ui.modal = SettingsModalState::Idle;
    session.settings_ui.clear_script_provider_transient_state();
    session.settings_ui.token_editing_provider = None;
    effects.push(ContextEffect::Render.into());
}

pub(super) fn script_provider_save_finished(
    session: &mut AppSession,
    completion: ScriptProviderSaveCompletion,
    effects: &mut Vec<AppEffect>,
) {
    let ScriptProviderSaveCompletion {
        request_id,
        config,
        yaml_filename,
        script_filename,
        is_editing,
        result,
    } = completion;
    let restore_submission_ui = session.settings_ui.settle_custom_provider_save(request_id);
    match result {
        Ok(success) => {
            let (title_key, body_key) = script_provider_ops::script_provider_save_notification_keys(
                is_editing,
                success.settings_saved,
            );
            super::shared::notify_plain_i18n(effects, title_key, body_key);
            effects.push(RefreshEffect::SendRequest(RefreshRequest::ReloadProviders).into());
        }
        Err(_failure) => {
            if is_editing && restore_submission_ui {
                script_provider_ops::rollback_script_provider_edit(
                    session,
                    &config,
                    &yaml_filename,
                    &script_filename,
                );
            } else if !is_editing {
                script_provider_ops::rollback_script_provider_create_registration(session, &config);
                if restore_submission_ui {
                    script_provider_ops::restore_script_provider_create_form(session);
                }
                effects.push(SettingsEffect::PersistSettings.into());
            }
            let (title_key, body_key) =
                script_provider_ops::script_provider_save_failed_notification_keys();
            super::shared::notify_plain_i18n(effects, title_key, body_key);
            effects.push(ContextEffect::Render.into());
        }
    }
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
    session.settings_ui.clear_script_provider_transient_state();
    session.settings_ui.token_editing_provider = None;
    session.settings_ui.modal = SettingsModalState::LoadingScriptProvider(provider_id.clone());
    effects.push(ScriptProviderEffect::LoadConfig { provider_id }.into());
    effects.push(ContextEffect::Render.into());
}

pub(super) fn script_provider_load_finished(
    session: &mut AppSession,
    provider_id: ProviderId,
    result: Result<ScriptProviderEditData, CustomProviderLifecycleFailure>,
    effects: &mut Vec<AppEffect>,
) {
    if !matches!(
        &session.settings_ui.modal,
        SettingsModalState::LoadingScriptProvider(expected) if expected == &provider_id
    ) {
        return;
    }
    match result {
        Ok(edit_data) => {
            session.settings_ui.modal = SettingsModalState::EditingScriptProvider(edit_data);
            session.settings_ui.script_provider_test_result = None;
        }
        Err(_failure) => {
            session.settings_ui.modal = SettingsModalState::Idle;
            super::shared::notify_plain_i18n(
                effects,
                "script_provider.load_failed_title",
                "script_provider.load_failed_body",
            );
        }
    }
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
    let request_id = session
        .settings_ui
        .begin_custom_provider_delete(provider_id.clone());
    effects.push(ContextEffect::Render.into());
    effects.push(
        ScriptProviderEffect::DeleteProvider {
            request_id,
            provider_id,
        }
        .into(),
    );
}

pub(super) fn script_provider_delete_finished(
    session: &mut AppSession,
    request_id: u64,
    provider_id: ProviderId,
    result: Result<ScriptProviderDeleteSuccess, CustomProviderLifecycleFailure>,
    effects: &mut Vec<AppEffect>,
) {
    if !session
        .settings_ui
        .settle_custom_provider_delete(request_id, &provider_id)
    {
        return;
    }
    match result {
        Ok(ScriptProviderDeleteSuccess::DeletedAll { .. }) => {
            super::shared::commit_deleted_provider(session, &provider_id, effects);
            effects.push(RefreshEffect::SendRequest(RefreshRequest::ReloadProviders).into());
        }
        Ok(ScriptProviderDeleteSuccess::DeletedYamlOnly { .. }) => {
            super::shared::commit_deleted_provider(session, &provider_id, effects);
            super::shared::notify_plain_i18n(
                effects,
                "script_provider.delete_partial_title",
                "script_provider.delete_partial_body",
            );
            effects.push(RefreshEffect::SendRequest(RefreshRequest::ReloadProviders).into());
        }
        Err(_failure) => {
            super::shared::notify_plain_i18n(
                effects,
                "script_provider.delete_failed_title",
                "script_provider.delete_failed_body",
            );
        }
    }
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
