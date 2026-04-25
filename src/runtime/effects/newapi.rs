use std::cell::RefCell;
use std::rc::Rc;

use log::{info, warn};

use crate::application::{newapi_ops, NewApiEffect};
use crate::models::ProviderId;
use crate::providers::custom::generator;
use crate::refresh::RefreshRequest;

use super::super::{newapi_io, AppState};

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
    let filename = original_filename.unwrap_or_else(|| generator::generate_filename(&config));

    match newapi_io::save_newapi_yaml(&config, &filename) {
        Ok(path) => {
            info!(target: "runtime", "saved custom provider YAML to {}", path.display());
            let s = state.borrow();
            let settings_saved = s.settings_writer.flush(s.session.settings.clone());
            drop(s);
            let (title_key, body_key) =
                newapi_ops::newapi_save_notification_keys(is_editing, settings_saved);
            let title = rust_i18n::t!(title_key).to_string();
            let body = rust_i18n::t!(body_key).to_string();
            crate::platform::notification::send_plain_notification(&title, &body);
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
        }
    }
}

fn delete_provider(state: &Rc<RefCell<AppState>>, provider_id: ProviderId) {
    match newapi_io::delete_newapi_yaml(&provider_id) {
        Ok(path) => {
            info!(target: "runtime", "deleted custom provider YAML: {}", path.display());
            let _ = super::refresh::send_request(state, RefreshRequest::ReloadProviders);
        }
        Err(err) => {
            warn!(target: "runtime", "{err}");
            let title = rust_i18n::t!("newapi.delete_failed_title").to_string();
            let body = rust_i18n::t!("newapi.delete_failed_body").to_string();
            crate::platform::notification::send_plain_notification(&title, &body);
        }
    }
}

fn load_config(state: &Rc<RefCell<AppState>>, provider_id: ProviderId) {
    if let ProviderId::Custom(ref custom_id) = provider_id {
        if let Some(edit_data) = generator::read_newapi_config(custom_id) {
            let mut s = state.borrow_mut();
            s.session.settings_ui.adding_newapi = true;
            s.session.settings_ui.editing_newapi = Some(edit_data);
        } else {
            warn!(
                target: "settings",
                "NewApiEffect::LoadConfig: failed to read config for {}",
                custom_id
            );
        }
    }
}
