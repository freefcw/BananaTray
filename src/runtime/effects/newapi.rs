use std::cell::RefCell;
use std::rc::Rc;

use log::{info, warn};

use crate::application::{AppAction, NewApiEffect};
use crate::models::{CustomProviderLifecycleFailure, NewApiConfig, NewApiSaveSuccess, ProviderId};
use crate::providers::custom::api;
use crate::runtime::settings_writer::DeferredSettingsFlush;

use super::super::AppState;

pub(super) fn run(state: &Rc<RefCell<AppState>>, effect: NewApiEffect) -> Vec<AppAction> {
    let settings = {
        let state = state.borrow();
        state
            .settings_writer
            .defer_flush(state.session.settings.clone())
    };
    let job = crate::runtime::CustomProviderJob::NewApi { effect, settings };
    match state.borrow().custom_provider_tx.try_send(job) {
        Ok(()) => Vec::new(),
        Err(err) => {
            let detail = format!("failed to queue custom-provider I/O: {err}");
            vec![err.into_inner().queue_failure(detail)]
        }
    }
}

pub(crate) fn execute(
    effect: NewApiEffect,
    settings: DeferredSettingsFlush,
    settings_writer: &crate::runtime::settings_writer::SettingsWriterHandle,
) -> AppAction {
    match effect {
        NewApiEffect::SaveProvider {
            request_id,
            config,
            original_filename,
            original_id,
            is_editing,
        } => save_provider(
            settings_writer,
            settings,
            request_id,
            config,
            original_filename,
            original_id,
            is_editing,
        ),
        NewApiEffect::DeleteProvider {
            request_id,
            provider_id,
        } => delete_provider(request_id, provider_id),
        NewApiEffect::LoadConfig { provider_id } => load_config(provider_id),
    }
}

fn save_provider(
    settings_writer: &crate::runtime::settings_writer::SettingsWriterHandle,
    settings: DeferredSettingsFlush,
    request_id: u64,
    config: NewApiConfig,
    original_filename: Option<String>,
    original_id: Option<String>,
    is_editing: bool,
) -> AppAction {
    let filename = original_filename.unwrap_or_else(|| api::generate_filename(&config));
    // 编辑保存时保持原始身份（original_id），新增时按 base_url + user_id 计算
    let result = match api::save_newapi_yaml(&config, &filename, original_id.as_deref()) {
        Ok(path) => {
            info!(target: "settings", "saved custom provider YAML to {}", path.display());
            let settings_saved = settings_writer.flush_deferred(settings);
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
        request_id,
        config,
        filename,
        original_id,
        is_editing,
        result,
    }
}

fn delete_provider(request_id: u64, provider_id: ProviderId) -> AppAction {
    let result = api::delete_newapi_yaml(&provider_id).map_err(|err| {
        warn!(target: "settings", "{err}");
        err
    });

    if let Ok(path) = &result {
        info!(target: "settings", "deleted custom provider YAML: {}", path.display());
    }

    AppAction::NewApiDeleteFinished {
        request_id,
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
