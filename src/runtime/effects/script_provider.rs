use std::cell::RefCell;
use std::rc::Rc;

use log::{info, warn};

use crate::application::{AppAction, ScriptProviderEffect};
use crate::models::{
    parse_script_stdout, CustomProviderLifecycleFailure, ProviderId, ScriptProviderConfig,
    ScriptProviderDeleteSuccess, ScriptProviderSaveSuccess, ScriptProviderTestResult,
};
use crate::providers::custom::api;
use crate::runtime::settings_writer::DeferredSettingsFlush;
use std::process::Command;
use std::time::Duration;

use super::super::AppState;

pub(super) fn run(state: &Rc<RefCell<AppState>>, effect: ScriptProviderEffect) -> Vec<AppAction> {
    if let ScriptProviderEffect::TestProvider { request_id, config } = effect {
        let job = crate::runtime::ScriptTestJob { request_id, config };
        return match state.borrow().script_test_tx.try_send(job) {
            Ok(()) => Vec::new(),
            Err(err) => {
                let detail = err.to_string();
                vec![err.into_inner().queue_failure(detail)]
            }
        };
    }

    let settings = {
        let state = state.borrow();
        state
            .settings_writer
            .defer_flush(state.session.settings.clone())
    };
    let job = crate::runtime::CustomProviderJob::ScriptProvider { effect, settings };
    match state.borrow().custom_provider_tx.try_send(job) {
        Ok(()) => Vec::new(),
        Err(err) => {
            let detail = format!("failed to queue custom-provider work: {err}");
            vec![err.into_inner().queue_failure(detail)]
        }
    }
}

pub(crate) fn execute(
    effect: ScriptProviderEffect,
    settings: DeferredSettingsFlush,
    settings_writer: &crate::runtime::settings_writer::SettingsWriterHandle,
) -> AppAction {
    match effect {
        ScriptProviderEffect::TestProvider { request_id, config } => {
            AppAction::ScriptProviderTestFinished {
                request_id,
                result: execute_script_test(&config),
            }
        }
        ScriptProviderEffect::SaveProvider {
            request_id,
            config,
            original_yaml_filename,
            original_script_filename,
            is_editing,
        } => save_provider(
            settings_writer,
            settings,
            request_id,
            config,
            original_yaml_filename,
            original_script_filename,
            is_editing,
        ),
        ScriptProviderEffect::DeleteProvider {
            request_id,
            provider_id,
        } => delete_provider(request_id, provider_id),
        ScriptProviderEffect::LoadConfig { provider_id } => load_config(provider_id),
    }
}

pub(crate) fn execute_script_test(config: &ScriptProviderConfig) -> ScriptProviderTestResult {
    match write_temp_script_and_run(config) {
        Ok(CommandTestOutput {
            stdout,
            stderr,
            success,
        }) => {
            if !success {
                return ScriptProviderTestResult {
                    success: false,
                    message: "script exited with a non-zero status".to_string(),
                    stdout,
                    stderr,
                    preview: None,
                };
            }
            match parse_script_stdout(&stdout) {
                Ok(preview) => ScriptProviderTestResult {
                    success: true,
                    message: "OK".to_string(),
                    stdout,
                    stderr,
                    preview: Some(preview),
                },
                Err(err) => ScriptProviderTestResult {
                    success: false,
                    message: err,
                    stdout,
                    stderr,
                    preview: None,
                },
            }
        }
        Err(err) => ScriptProviderTestResult {
            success: false,
            message: err,
            stdout: String::new(),
            stderr: String::new(),
            preview: None,
        },
    }
}

struct CommandTestOutput {
    stdout: String,
    stderr: String,
    success: bool,
}

struct TempScriptDir {
    path: std::path::PathBuf,
}

impl TempScriptDir {
    fn create() -> Result<Self, String> {
        let path = std::env::temp_dir().join(format!(
            "bananatray-script-test-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::create_dir_all(&path).map_err(|e| format!("failed to create temp dir: {e}"))?;
        Ok(Self { path })
    }
}

impl Drop for TempScriptDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn write_temp_script_and_run(config: &ScriptProviderConfig) -> Result<CommandTestOutput, String> {
    let dir = TempScriptDir::create()?;
    let script_path = dir.path.join("provider_script.py");
    std::fs::write(&script_path, &config.script)
        .map_err(|e| format!("failed to write temp script: {e}"))?;

    let script_arg = script_path.to_string_lossy().to_string();
    let output = run_script_command(
        &config.interpreter,
        script_arg.as_str(),
        Duration::from_millis(config.timeout_ms.max(1)),
    )?;

    Ok(CommandTestOutput {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

fn run_script_command(
    interpreter: &str,
    script_path: &str,
    timeout: Duration,
) -> Result<std::process::Output, String> {
    let executable = crate::providers::common::path_resolver::locate_executable(interpreter)
        .ok_or_else(|| format!("interpreter not found: {interpreter}"))?;
    let mut command = Command::new(executable);
    command.arg(script_path).env(
        "PATH",
        crate::providers::common::path_resolver::enriched_path(),
    );
    crate::providers::common::cli::run_prepared_command_with_timeout(command, timeout)
        .map_err(|err| err.to_string())
}

fn save_provider(
    settings_writer: &crate::runtime::settings_writer::SettingsWriterHandle,
    settings: DeferredSettingsFlush,
    request_id: u64,
    config: ScriptProviderConfig,
    original_yaml_filename: Option<String>,
    original_script_filename: Option<String>,
    is_editing: bool,
) -> AppAction {
    let yaml_filename =
        original_yaml_filename.unwrap_or_else(|| api::generate_script_yaml_filename(&config));
    let script_filename =
        original_script_filename.unwrap_or_else(|| api::generate_script_filename(&config));

    let result =
        match api::save_script_provider(&config, &yaml_filename, &script_filename, is_editing) {
            Ok((yaml_path, script_path)) => {
                info!(
                    target: "settings",
                    "saved script provider YAML to {}, script to {}",
                    yaml_path.display(),
                    script_path.display()
                );
                let settings_saved = settings_writer.flush_deferred(settings);
                Ok(ScriptProviderSaveSuccess {
                    yaml_path,
                    script_path,
                    settings_saved,
                })
            }
            Err(err) => {
                warn!(target: "settings", "failed to save script provider: {}", err);
                Err(err)
            }
        };

    AppAction::ScriptProviderSaveFinished {
        request_id,
        config,
        yaml_filename,
        script_filename,
        is_editing,
        result,
    }
}

fn delete_provider(request_id: u64, provider_id: ProviderId) -> AppAction {
    let result = match api::delete_script_provider_files(&provider_id) {
        Ok((yaml_path, script_result)) => match script_result {
            Ok(script_path) => {
                info!(
                    target: "settings",
                    "deleted script provider files: {}, {}",
                    yaml_path.display(),
                    script_path.display()
                );
                Ok(ScriptProviderDeleteSuccess::DeletedAll {
                    yaml_path,
                    script_path,
                })
            }
            Err(err) => {
                warn!(
                    target: "settings",
                    "deleted script provider YAML {}, but failed to delete companion script: {}",
                    yaml_path.display(),
                    err
                );
                Ok(ScriptProviderDeleteSuccess::DeletedYamlOnly {
                    yaml_path,
                    script_failure: err,
                })
            }
        },
        Err(err) => {
            warn!(target: "settings", "{err}");
            Err(err)
        }
    };

    AppAction::ScriptProviderDeleteFinished {
        request_id,
        provider_id,
        result,
    }
}

fn load_config(provider_id: ProviderId) -> AppAction {
    let result = match &provider_id {
        ProviderId::Custom(custom_id) => {
            api::read_script_provider_config(custom_id).ok_or_else(|| {
                let failure = CustomProviderLifecycleFailure::yaml_not_found(
                    "load script provider",
                    custom_id,
                    None,
                );
                warn!(
                    target: "settings",
                    "ScriptProviderEffect::LoadConfig: {}",
                    failure
                );
                failure
            })
        }
        _ => {
            let failure = CustomProviderLifecycleFailure::invalid_provider_id(
                "load script provider",
                "custom",
                provider_id.to_string(),
            );
            warn!(target: "settings", "ScriptProviderEffect::LoadConfig: {}", failure);
            Err(failure)
        }
    };

    AppAction::ScriptProviderLoadFinished {
        provider_id,
        result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config(script: &str) -> ScriptProviderConfig {
        ScriptProviderConfig {
            display_name: "Script".to_string(),
            provider_id: "script:script".to_string(),
            interpreter: "sh".to_string(),
            timeout_ms: 1_000,
            script: script.to_string(),
        }
    }

    #[test]
    fn execute_script_test_parses_valid_stdout_json() {
        let result = execute_script_test(&make_config(
            r#"printf '{"label":"Balance","remaining":"12.5","used":2,"unit":"USD"}'"#,
        ));

        assert!(result.success, "unexpected failure: {}", result.message);
        let preview = result.preview.expect("preview");
        assert_eq!(preview.label, "Balance");
        assert_eq!(preview.remaining, 12.5);
        assert_eq!(preview.used, Some(2.0));
        assert_eq!(preview.unit, "USD");
    }

    #[test]
    fn execute_script_test_reports_nonzero_exit() {
        let result = execute_script_test(&make_config("printf 'boom' >&2\nexit 3"));

        assert!(!result.success);
        assert!(result.message.contains("non-zero"));
        assert_eq!(result.stderr, "boom");
        assert!(result.preview.is_none());
    }
}
