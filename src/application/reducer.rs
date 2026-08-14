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

/// 将 action 穷尽分派到对应领域 reducer。
///
/// 这里直接解构 payload，避免先按家族转发后再用 `unreachable!` 做第二次 match。
/// 新增 `AppAction` 变体时，编译器会要求在此补齐唯一的分派入口。
pub fn reduce(session: &mut AppSession, action: AppAction) -> Vec<AppEffect> {
    let mut effects = Vec::new();

    match action {
        AppAction::SelectNavTab(tab) => settings::select_nav_tab(session, tab, &mut effects),
        AppAction::SetSettingsTab(tab) => settings::set_settings_tab(session, tab, &mut effects),
        AppAction::ToggleCadenceDropdown => {
            settings::toggle_cadence_dropdown(session, &mut effects)
        }
        AppAction::SaveGlobalHotkey(hotkey) => {
            settings::save_global_hotkey(session, hotkey, &mut effects)
        }
        AppAction::SaveTrayPopupPosition(position) => {
            settings::save_tray_popup_position(session, position, &mut effects)
        }
        AppAction::GlobalHotkeyApplyFinished { requested, result } => {
            settings::finish_global_hotkey_apply(session, requested, result, &mut effects)
        }
        AppAction::UpdateSetting(change) => {
            settings::apply_setting_change(session, change, &mut effects)
        }
        AppAction::OpenSettings { provider } => {
            settings::open_settings(session, provider, &mut effects)
        }
        AppAction::OpenUrl(url) => settings::open_url(url, &mut effects),
        AppAction::PopupVisibilityChanged(visible) => {
            settings::popup_visibility_changed(session, visible, &mut effects)
        }
        AppAction::QuitApp => settings::quit_app(&mut effects),

        AppAction::RefreshProvider { id, reason } => {
            refresh::request_provider_refresh(session, id, reason, &mut effects)
        }
        AppAction::RefreshAll => refresh::refresh_all_providers(session, &mut effects),
        AppAction::RefreshEventReceived(event) => {
            refresh::apply_refresh_event(session, event, &mut effects)
        }

        AppAction::UpdateLogLevel(level) => debug::update_log_level(level, &mut effects),
        AppAction::SendDebugNotification(kind) => {
            debug::send_debug_notification(session, kind, &mut effects)
        }
        AppAction::OpenLogDirectory => debug::open_log_directory(&mut effects),
        AppAction::CopyToClipboard(text) => debug::copy_to_clipboard(text, &mut effects),
        AppAction::SelectDebugProvider(id) => {
            debug::select_debug_provider(session, id, &mut effects)
        }
        AppAction::DebugRefreshProvider => debug::debug_refresh_provider(session, &mut effects),
        AppAction::ClearDebugLogs => debug::clear_debug_logs(&mut effects),

        AppAction::SelectSettingsProvider(id) => {
            provider_sidebar::select_settings_provider(session, id, &mut effects)
        }
        AppAction::SetTokenEditing {
            provider_id,
            editing,
        } => provider_sidebar::set_token_editing(session, provider_id, editing, &mut effects),
        AppAction::SaveProviderToken { provider_id, token } => {
            provider_sidebar::save_provider_token(session, provider_id, token, &mut effects)
        }
        AppAction::MoveProviderToIndex { id, target_index } => {
            provider_sidebar::move_provider_to_index(session, id, target_index, &mut effects)
        }
        AppAction::ToggleProvider(id) => {
            provider_sidebar::toggle_provider(session, id, &mut effects)
        }
        AppAction::OpenDashboard(id) => provider_sidebar::open_dashboard(session, id, &mut effects),
        AppAction::EnterAddProvider => provider_sidebar::enter_add_provider(session, &mut effects),
        AppAction::CancelAddProvider => {
            provider_sidebar::cancel_add_provider(session, &mut effects)
        }
        AppAction::AddProviderToSidebar(id) => {
            provider_sidebar::add_provider_to_sidebar(session, id, &mut effects)
        }
        AppAction::RemoveProviderFromSidebar(id) => {
            provider_sidebar::remove_provider_from_sidebar(session, id, &mut effects)
        }
        AppAction::ConfirmRemoveProvider => {
            provider_sidebar::confirm_remove_provider(session, &mut effects)
        }
        AppAction::CancelRemoveProvider => {
            provider_sidebar::cancel_remove_provider(session, &mut effects)
        }

        AppAction::EnterAddNewApi => newapi::enter_add_newapi(session, &mut effects),
        AppAction::CancelAddNewApi => newapi::cancel_add_newapi(session, &mut effects),
        AppAction::SubmitNewApi(config) => newapi::submit_newapi(session, config, &mut effects),
        AppAction::NewApiSaveFinished {
            config,
            filename,
            original_id,
            is_editing,
            result,
        } => newapi::newapi_save_finished(
            session,
            config,
            filename,
            original_id,
            is_editing,
            result,
            &mut effects,
        ),
        AppAction::EditNewApi { provider_id } => {
            newapi::edit_newapi(session, provider_id, &mut effects)
        }
        AppAction::NewApiLoadFinished {
            provider_id,
            result,
        } => newapi::newapi_load_finished(session, provider_id, result, &mut effects),
        AppAction::DeleteNewApi { provider_id } => {
            newapi::delete_newapi(session, provider_id, &mut effects)
        }
        AppAction::NewApiDeleteFinished {
            provider_id,
            result,
        } => newapi::newapi_delete_finished(session, provider_id, result, &mut effects),
        AppAction::ConfirmDeleteNewApi => newapi::confirm_delete_newapi(session, &mut effects),
        AppAction::CancelDeleteNewApi => newapi::cancel_delete_newapi(session, &mut effects),

        AppAction::EnterAddScriptProvider => {
            script_provider::enter_add_script_provider(session, &mut effects)
        }
        AppAction::CancelAddScriptProvider => {
            script_provider::cancel_add_script_provider(session, &mut effects)
        }
        AppAction::TestScriptProvider(config) => {
            script_provider::test_script_provider(session, config, &mut effects)
        }
        AppAction::ScriptProviderTestFinished { request_id, result } => {
            script_provider::script_provider_test_finished(
                session,
                request_id,
                result,
                &mut effects,
            )
        }
        AppAction::SubmitScriptProvider(config) => {
            script_provider::submit_script_provider(session, config, &mut effects)
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
            &mut effects,
        ),
        AppAction::EditScriptProvider { provider_id } => {
            script_provider::edit_script_provider(session, provider_id, &mut effects)
        }
        AppAction::ScriptProviderLoadFinished {
            provider_id,
            result,
        } => script_provider::script_provider_load_finished(
            session,
            provider_id,
            result,
            &mut effects,
        ),
        AppAction::DeleteScriptProvider { provider_id } => {
            script_provider::delete_script_provider(session, provider_id, &mut effects)
        }
        AppAction::ScriptProviderDeleteFinished {
            provider_id,
            result,
        } => script_provider::script_provider_delete_finished(
            session,
            provider_id,
            result,
            &mut effects,
        ),
        AppAction::ConfirmDeleteScriptProvider => {
            script_provider::confirm_delete_script_provider(session, &mut effects)
        }
        AppAction::CancelDeleteScriptProvider => {
            script_provider::cancel_delete_script_provider(session, &mut effects)
        }
    }

    effects
}

#[cfg(test)]
#[path = "reducer_tests/mod.rs"]
mod tests;
