use std::cell::RefCell;
use std::rc::Rc;

use log::{info, warn};

use crate::application::{AppAction, NewApiEffect};
use crate::models::{CustomProviderLifecycleFailure, NewApiConfig, NewApiSaveSuccess, ProviderId};
use crate::providers::custom::api;

use super::super::AppState;

pub(super) fn run(state: &Rc<RefCell<AppState>>, effect: NewApiEffect) -> Vec<AppAction> {
    match effect {
        NewApiEffect::SaveProvider {
            config,
            original_filename,
            is_editing,
        } => vec![save_provider(state, config, original_filename, is_editing)],
        NewApiEffect::DeleteProvider { provider_id } => {
            vec![delete_provider(provider_id)]
        }
        NewApiEffect::LoadConfig { provider_id } => vec![load_config(provider_id)],
    }
}

fn save_provider(
    state: &Rc<RefCell<AppState>>,
    config: NewApiConfig,
    original_filename: Option<String>,
    is_editing: bool,
) -> AppAction {
    let filename = original_filename.unwrap_or_else(|| api::generate_filename(&config));
    let result = match api::save_newapi_yaml(&config, &filename) {
        Ok(path) => {
            info!(target: "settings", "saved custom provider YAML to {}", path.display());
            let s = state.borrow();
            let settings_saved = s.settings_writer.flush(s.session.settings.clone());
            Ok(NewApiSaveSuccess {
                path,
                settings_saved,
            })
        }
        Err(err) => {
            warn!(target: "settings", "failed to save newapi: {}", err);
            Err(err)
        }
    };

    AppAction::NewApiSaveFinished {
        config,
        filename,
        is_editing,
        result,
    }
}

fn delete_provider(provider_id: ProviderId) -> AppAction {
    let result = api::delete_newapi_yaml(&provider_id).map_err(|err| {
        warn!(target: "settings", "{err}");
        err
    });

    if let Ok(path) = &result {
        info!(target: "settings", "deleted custom provider YAML: {}", path.display());
    }

    AppAction::NewApiDeleteFinished {
        provider_id,
        result,
    }
}

fn load_config(provider_id: ProviderId) -> AppAction {
    let result = match &provider_id {
        ProviderId::Custom(custom_id) => api::read_newapi_config(custom_id).ok_or_else(|| {
            let failure = CustomProviderLifecycleFailure::yaml_not_found(
                "load NewAPI provider",
                custom_id,
                None,
            );
            warn!(
                target: "settings",
                "NewApiEffect::LoadConfig: {}",
                failure
            );
            failure
        }),
        _ => {
            let failure = CustomProviderLifecycleFailure::invalid_provider_id(
                "load NewAPI provider",
                "custom",
                provider_id.to_string(),
            );
            warn!(target: "settings", "NewApiEffect::LoadConfig: {}", failure);
            Err(failure)
        }
    };

    AppAction::NewApiLoadFinished {
        provider_id,
        result,
    }
}
