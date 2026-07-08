use std::cell::RefCell;
use std::rc::Rc;

use log::{info, warn};

use crate::application::{newapi_ops, NewApiEffect, SettingsModalState};
use crate::models::ProviderId;
use crate::providers::custom::api;
use crate::refresh::RefreshRequest;

use super::super::AppState;

pub(super) fn run(state: &Rc<RefCell<AppState>>, effect: NewApiEffect) {
    match effect {
        NewApiEffect::SaveProvider {
            config,
            original_filename,
            is_editing,
        } => save_provider(state, config, original_filename, is_editing),
        NewApiEffect::DeleteProvider { provider_id } => delete_provider(state, provider_id),
        NewApiEffect::LoadConfig { provider_id } => load_config(state, provider_id),
    }
}

fn save_provider(
    state: &Rc<RefCell<AppState>>,
    config: crate::models::NewApiConfig,
    original_filename: Option<String>,
    is_editing: bool,
) {
    let filename = original_filename.unwrap_or_else(|| api::generate_filename(&config));

    match api::save_newapi_yaml(&config, &filename) {
        Ok(path) => {
            info!(target: "runtime", "saved custom provider YAML to {}", path.display());
            let s = state.borrow();
            let settings_saved = s.settings_writer.flush(s.session.settings.clone());
            drop(s);
            let (title_key, body_key) =
                newapi_ops::newapi_save_notification_keys(is_editing, settings_saved);
            super::notification::notify_plain_i18n(title_key, body_key);
            let _ = super::refresh::send_request(state, RefreshRequest::ReloadProviders);
        }
        Err(e) => {
            warn!(target: "runtime", "failed to save newapi: {}", e);
            let mut s = state.borrow_mut();
            if is_editing {
                newapi_ops::rollback_newapi_edit(&mut s.session, &config, &filename);
            } else {
                newapi_ops::rollback_newapi_create(&mut s.session, &config);
            }
            drop(s);
            let (title_key, body_key) = newapi_ops::newapi_save_failed_notification_keys();
            super::notification::notify_plain_i18n(title_key, body_key);
        }
    }
}

fn delete_provider(state: &Rc<RefCell<AppState>>, provider_id: ProviderId) {
    match api::delete_newapi_yaml(&provider_id) {
        Ok(path) => {
            info!(target: "runtime", "deleted custom provider YAML: {}", path.display());
            let _ = super::refresh::send_request(state, RefreshRequest::ReloadProviders);
        }
        Err(err) => {
            warn!(target: "runtime", "{err}");
            super::notification::notify_plain_i18n(
                "newapi.delete_failed_title",
                "newapi.delete_failed_body",
            );
        }
    }
}

fn load_config(state: &Rc<RefCell<AppState>>, provider_id: ProviderId) {
    if let ProviderId::Custom(ref custom_id) = provider_id {
        if let Some(edit_data) = api::read_newapi_config(custom_id) {
            let mut s = state.borrow_mut();
            s.session.settings_ui.modal = SettingsModalState::EditingNewApi(edit_data);
        } else {
            warn!(
                target: "settings",
                "NewApiEffect::LoadConfig: failed to read config for {}",
                custom_id
            );
        }
    }
}
