mod debug;
mod newapi;
mod provider_sidebar;
mod refresh;
mod settings;
mod shared;

use super::state::AppSession;
use crate::application::{AppAction, AppEffect};

pub use shared::build_config_sync_request;

pub fn reduce(session: &mut AppSession, action: AppAction) -> Vec<AppEffect> {
    let mut effects = Vec::new();

    match action {
        AppAction::SelectNavTab(tab) => settings::select_nav_tab(session, tab, &mut effects),
        AppAction::SetSettingsTab(tab) => settings::set_settings_tab(session, tab, &mut effects),
        AppAction::SelectSettingsProvider(id) => {
            provider_sidebar::select_settings_provider(session, id, &mut effects);
        }
        AppAction::ToggleCadenceDropdown => {
            settings::toggle_cadence_dropdown(session, &mut effects);
        }
        AppAction::SetTokenEditing {
            provider_id,
            editing,
        } => provider_sidebar::set_token_editing(session, provider_id, editing, &mut effects),
        AppAction::SaveProviderToken { provider_id, token } => {
            provider_sidebar::save_provider_token(session, provider_id, token, &mut effects);
        }
        AppAction::MoveProviderToIndex { id, target_index } => {
            provider_sidebar::move_provider_to_index(session, id, target_index, &mut effects);
        }
        AppAction::SaveGlobalHotkey(hotkey) => {
            settings::save_global_hotkey(session, hotkey, &mut effects);
        }
        AppAction::UpdateSetting(change) => {
            settings::apply_setting_change(session, change, &mut effects);
        }
        AppAction::RefreshProvider { id, reason } => {
            refresh::request_provider_refresh(session, id, reason, &mut effects);
        }
        AppAction::RefreshAll => refresh::refresh_all_providers(session, &mut effects),
        AppAction::ToggleProvider(id) => {
            provider_sidebar::toggle_provider(session, id, &mut effects);
        }
        AppAction::RefreshEventReceived(event) => {
            refresh::apply_refresh_event(session, event, &mut effects);
        }
        AppAction::OpenSettings { provider } => {
            settings::open_settings(session, provider, &mut effects);
        }
        AppAction::OpenDashboard(id) => {
            provider_sidebar::open_dashboard(session, id, &mut effects);
        }
        AppAction::OpenUrl(url) => settings::open_url(url, &mut effects),
        AppAction::UpdateLogLevel(level) => debug::update_log_level(level, &mut effects),
        AppAction::SendDebugNotification(kind) => {
            debug::send_debug_notification(session, kind, &mut effects);
        }
        AppAction::OpenLogDirectory => debug::open_log_directory(&mut effects),
        AppAction::CopyToClipboard(text) => debug::copy_to_clipboard(text, &mut effects),
        AppAction::SelectDebugProvider(id) => {
            debug::select_debug_provider(session, id, &mut effects);
        }
        AppAction::DebugRefreshProvider => debug::debug_refresh_provider(session, &mut effects),
        AppAction::ClearDebugLogs => debug::clear_debug_logs(&mut effects),
        AppAction::PopupVisibilityChanged(visible) => {
            settings::popup_visibility_changed(session, visible, &mut effects);
        }
        AppAction::EnterAddProvider => provider_sidebar::enter_add_provider(session, &mut effects),
        AppAction::CancelAddProvider => {
            provider_sidebar::cancel_add_provider(session, &mut effects);
        }
        AppAction::AddProviderToSidebar(id) => {
            provider_sidebar::add_provider_to_sidebar(session, id, &mut effects);
        }
        AppAction::RemoveProviderFromSidebar(id) => {
            provider_sidebar::remove_provider_from_sidebar(session, id, &mut effects);
        }
        AppAction::ConfirmRemoveProvider => {
            provider_sidebar::confirm_remove_provider(session, &mut effects);
        }
        AppAction::CancelRemoveProvider => {
            provider_sidebar::cancel_remove_provider(session, &mut effects);
        }
        AppAction::EnterAddNewApi => newapi::enter_add_newapi(session, &mut effects),
        AppAction::CancelAddNewApi => newapi::cancel_add_newapi(session, &mut effects),
        AppAction::SubmitNewApi {
            display_name,
            base_url,
            cookie,
            user_id,
            divisor,
        } => newapi::submit_newapi(
            session,
            display_name,
            base_url,
            cookie,
            user_id,
            divisor,
            &mut effects,
        ),
        AppAction::EditNewApi { provider_id } => {
            newapi::edit_newapi(provider_id, &mut effects);
        }
        AppAction::DeleteNewApi { provider_id } => {
            newapi::delete_newapi(session, provider_id, &mut effects);
        }
        AppAction::ConfirmDeleteNewApi => newapi::confirm_delete_newapi(session, &mut effects),
        AppAction::CancelDeleteNewApi => newapi::cancel_delete_newapi(session, &mut effects),
        AppAction::QuitApp => settings::quit_app(&mut effects),
    }

    effects
}

#[cfg(test)]
#[path = "reducer_tests.rs"]
mod tests;
