mod debug;
mod newapi;
mod provider_sidebar;
mod refresh;
mod script_provider;
mod settings;
mod shared;

use super::state::AppSession;
use crate::application::{AppAction, AppEffect};

pub use shared::build_config_sync_request;

pub fn reduce(session: &mut AppSession, action: AppAction) -> Vec<AppEffect> {
    let mut effects = Vec::new();

    match action {
        action @ (AppAction::SelectNavTab(_)
        | AppAction::SetSettingsTab(_)
        | AppAction::ToggleCadenceDropdown
        | AppAction::SaveGlobalHotkey(_)
        | AppAction::UpdateSetting(_)
        | AppAction::OpenSettings { .. }
        | AppAction::OpenUrl(_)
        | AppAction::PopupVisibilityChanged(_)
        | AppAction::QuitApp) => reduce_settings_action(session, action, &mut effects),
        action @ (AppAction::RefreshProvider { .. }
        | AppAction::RefreshAll
        | AppAction::RefreshEventReceived(_)) => {
            reduce_refresh_action(session, action, &mut effects);
        }
        action @ (AppAction::UpdateLogLevel(_)
        | AppAction::SendDebugNotification(_)
        | AppAction::OpenLogDirectory
        | AppAction::CopyToClipboard(_)
        | AppAction::SelectDebugProvider(_)
        | AppAction::DebugRefreshProvider
        | AppAction::ClearDebugLogs) => reduce_debug_action(session, action, &mut effects),
        action @ (AppAction::SelectSettingsProvider(_)
        | AppAction::SetTokenEditing { .. }
        | AppAction::SaveProviderToken { .. }
        | AppAction::MoveProviderToIndex { .. }
        | AppAction::ToggleProvider(_)
        | AppAction::OpenDashboard(_)
        | AppAction::EnterAddProvider
        | AppAction::CancelAddProvider
        | AppAction::AddProviderToSidebar(_)
        | AppAction::RemoveProviderFromSidebar(_)
        | AppAction::ConfirmRemoveProvider
        | AppAction::CancelRemoveProvider) => {
            reduce_provider_action(session, action, &mut effects);
        }
        action @ (AppAction::EnterAddNewApi
        | AppAction::CancelAddNewApi
        | AppAction::SubmitNewApi(_)
        | AppAction::NewApiSaveFinished { .. }
        | AppAction::EditNewApi { .. }
        | AppAction::NewApiLoadFinished { .. }
        | AppAction::DeleteNewApi { .. }
        | AppAction::NewApiDeleteFinished { .. }
        | AppAction::ConfirmDeleteNewApi
        | AppAction::CancelDeleteNewApi) => {
            reduce_newapi_action(session, action, &mut effects);
        }
        action @ (AppAction::EnterAddScriptProvider
        | AppAction::CancelAddScriptProvider
        | AppAction::TestScriptProvider(_)
        | AppAction::ScriptProviderTestFinished { .. }
        | AppAction::SubmitScriptProvider(_)
        | AppAction::ScriptProviderSaveFinished { .. }
        | AppAction::EditScriptProvider { .. }
        | AppAction::ScriptProviderLoadFinished { .. }
        | AppAction::DeleteScriptProvider { .. }
        | AppAction::ScriptProviderDeleteFinished { .. }
        | AppAction::ConfirmDeleteScriptProvider
        | AppAction::CancelDeleteScriptProvider) => {
            reduce_script_provider_action(session, action, &mut effects);
        }
    }

    effects
}

fn reduce_settings_action(
    session: &mut AppSession,
    action: AppAction,
    effects: &mut Vec<AppEffect>,
) {
    match action {
        AppAction::SelectNavTab(tab) => settings::select_nav_tab(session, tab, effects),
        AppAction::SetSettingsTab(tab) => settings::set_settings_tab(session, tab, effects),
        AppAction::ToggleCadenceDropdown => settings::toggle_cadence_dropdown(session, effects),
        AppAction::SaveGlobalHotkey(hotkey) => {
            settings::save_global_hotkey(session, hotkey, effects);
        }
        AppAction::UpdateSetting(change) => {
            settings::apply_setting_change(session, change, effects);
        }
        AppAction::OpenSettings { provider } => settings::open_settings(session, provider, effects),
        AppAction::OpenUrl(url) => settings::open_url(url, effects),
        AppAction::PopupVisibilityChanged(visible) => {
            settings::popup_visibility_changed(session, visible, effects);
        }
        AppAction::QuitApp => settings::quit_app(effects),
        _ => unreachable!("settings action dispatcher received another action family"),
    }
}

fn reduce_refresh_action(
    session: &mut AppSession,
    action: AppAction,
    effects: &mut Vec<AppEffect>,
) {
    match action {
        AppAction::RefreshProvider { id, reason } => {
            refresh::request_provider_refresh(session, id, reason, effects);
        }
        AppAction::RefreshAll => refresh::refresh_all_providers(session, effects),
        AppAction::RefreshEventReceived(event) => {
            refresh::apply_refresh_event(session, event, effects);
        }
        _ => unreachable!("refresh action dispatcher received another action family"),
    }
}

fn reduce_debug_action(session: &mut AppSession, action: AppAction, effects: &mut Vec<AppEffect>) {
    match action {
        AppAction::UpdateLogLevel(level) => debug::update_log_level(level, effects),
        AppAction::SendDebugNotification(kind) => {
            debug::send_debug_notification(session, kind, effects);
        }
        AppAction::OpenLogDirectory => debug::open_log_directory(effects),
        AppAction::CopyToClipboard(text) => debug::copy_to_clipboard(text, effects),
        AppAction::SelectDebugProvider(id) => {
            debug::select_debug_provider(session, id, effects);
        }
        AppAction::DebugRefreshProvider => debug::debug_refresh_provider(session, effects),
        AppAction::ClearDebugLogs => debug::clear_debug_logs(effects),
        _ => unreachable!("debug action dispatcher received another action family"),
    }
}

fn reduce_provider_action(
    session: &mut AppSession,
    action: AppAction,
    effects: &mut Vec<AppEffect>,
) {
    match action {
        AppAction::SelectSettingsProvider(id) => {
            provider_sidebar::select_settings_provider(session, id, effects);
        }
        AppAction::SetTokenEditing {
            provider_id,
            editing,
        } => provider_sidebar::set_token_editing(session, provider_id, editing, effects),
        AppAction::SaveProviderToken { provider_id, token } => {
            provider_sidebar::save_provider_token(session, provider_id, token, effects);
        }
        AppAction::MoveProviderToIndex { id, target_index } => {
            provider_sidebar::move_provider_to_index(session, id, target_index, effects);
        }
        AppAction::ToggleProvider(id) => provider_sidebar::toggle_provider(session, id, effects),
        AppAction::OpenDashboard(id) => provider_sidebar::open_dashboard(session, id, effects),
        AppAction::EnterAddProvider => provider_sidebar::enter_add_provider(session, effects),
        AppAction::CancelAddProvider => provider_sidebar::cancel_add_provider(session, effects),
        AppAction::AddProviderToSidebar(id) => {
            provider_sidebar::add_provider_to_sidebar(session, id, effects);
        }
        AppAction::RemoveProviderFromSidebar(id) => {
            provider_sidebar::remove_provider_from_sidebar(session, id, effects);
        }
        AppAction::ConfirmRemoveProvider => {
            provider_sidebar::confirm_remove_provider(session, effects);
        }
        AppAction::CancelRemoveProvider => {
            provider_sidebar::cancel_remove_provider(session, effects);
        }
        _ => unreachable!("provider action dispatcher received another action family"),
    }
}

fn reduce_newapi_action(session: &mut AppSession, action: AppAction, effects: &mut Vec<AppEffect>) {
    match action {
        AppAction::EnterAddNewApi => newapi::enter_add_newapi(session, effects),
        AppAction::CancelAddNewApi => newapi::cancel_add_newapi(session, effects),
        AppAction::SubmitNewApi(config) => newapi::submit_newapi(session, config, effects),
        AppAction::NewApiSaveFinished {
            config,
            filename,
            is_editing,
            result,
        } => newapi::newapi_save_finished(session, config, filename, is_editing, result, effects),
        AppAction::EditNewApi { provider_id } => {
            newapi::edit_newapi(session, provider_id, effects);
        }
        AppAction::NewApiLoadFinished {
            provider_id,
            result,
        } => newapi::newapi_load_finished(session, provider_id, result, effects),
        AppAction::DeleteNewApi { provider_id } => {
            newapi::delete_newapi(session, provider_id, effects);
        }
        AppAction::NewApiDeleteFinished {
            provider_id,
            result,
        } => newapi::newapi_delete_finished(session, provider_id, result, effects),
        AppAction::ConfirmDeleteNewApi => newapi::confirm_delete_newapi(session, effects),
        AppAction::CancelDeleteNewApi => newapi::cancel_delete_newapi(session, effects),
        _ => unreachable!("NewAPI action dispatcher received another action family"),
    }
}

fn reduce_script_provider_action(
    session: &mut AppSession,
    action: AppAction,
    effects: &mut Vec<AppEffect>,
) {
    match action {
        AppAction::EnterAddScriptProvider => {
            script_provider::enter_add_script_provider(session, effects);
        }
        AppAction::CancelAddScriptProvider => {
            script_provider::cancel_add_script_provider(session, effects);
        }
        AppAction::TestScriptProvider(config) => {
            script_provider::test_script_provider(session, config, effects);
        }
        AppAction::ScriptProviderTestFinished { request_id, result } => {
            script_provider::script_provider_test_finished(session, request_id, result, effects);
        }
        AppAction::SubmitScriptProvider(config) => {
            script_provider::submit_script_provider(session, config, effects);
        }
        AppAction::ScriptProviderSaveFinished {
            config,
            yaml_filename,
            script_filename,
            is_editing,
            result,
        } => script_provider::script_provider_save_finished(
            session,
            config,
            yaml_filename,
            script_filename,
            is_editing,
            result,
            effects,
        ),
        AppAction::EditScriptProvider { provider_id } => {
            script_provider::edit_script_provider(session, provider_id, effects);
        }
        AppAction::ScriptProviderLoadFinished {
            provider_id,
            result,
        } => script_provider::script_provider_load_finished(session, provider_id, result, effects),
        AppAction::DeleteScriptProvider { provider_id } => {
            script_provider::delete_script_provider(session, provider_id, effects);
        }
        AppAction::ScriptProviderDeleteFinished {
            provider_id,
            result,
        } => {
            script_provider::script_provider_delete_finished(session, provider_id, result, effects)
        }
        AppAction::ConfirmDeleteScriptProvider => {
            script_provider::confirm_delete_script_provider(session, effects);
        }
        AppAction::CancelDeleteScriptProvider => {
            script_provider::cancel_delete_script_provider(session, effects);
        }
        _ => unreachable!("script provider dispatcher received another action family"),
    }
}

#[cfg(test)]
#[path = "reducer_tests/mod.rs"]
mod tests;
