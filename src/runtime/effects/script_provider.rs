use std::cell::RefCell;
use std::rc::Rc;

use log::{info, warn};

use crate::application::{script_provider_ops, ScriptProviderEffect};
use crate::models::{
    parse_script_stdout, ProviderId, ScriptProviderConfig, ScriptProviderTestResult,
};
use crate::providers::custom::api;
use crate::refresh::RefreshRequest;
use std::process::Command;
use std::time::Duration;

use super::super::AppState;

pub(super) fn run(state: &Rc<RefCell<AppState>>, effect: ScriptProviderEffect) {
    match effect {
        ScriptProviderEffect::TestProvider { request_id, config } => {
            test_provider(state, request_id, config)
        }
        ScriptProviderEffect::SaveProvider {
            config,
            original_yaml_filename,
            original_script_filename,
            is_editing,
        } => save_provider(
            state,
            config,
            original_yaml_filename,
            original_script_filename,
            is_editing,
        ),
        ScriptProviderEffect::DeleteProvider { provider_id } => delete_provider(state, provider_id),
        ScriptProviderEffect::LoadConfig { provider_id } => load_config(state, provider_id),
    }
}

fn test_provider(state: &Rc<RefCell<AppState>>, request_id: u64, config: ScriptProviderConfig) {
    let send_result = state.borrow().script_test_tx.try_send((request_id, config));
    if let Err(err) = send_result {
        let mut state = state.borrow_mut();
        if state
            .session
            .settings_ui
            .script_provider_pending_test_request_id
            == Some(request_id)
        {
            state.session.settings_ui.script_provider_testing = false;
            state
                .session
                .settings_ui
                .script_provider_pending_test_request_id = None;
            state.session.settings_ui.script_provider_test_result =
                Some(ScriptProviderTestResult {
                    success: false,
                    message: format!("failed to queue script test: {err}"),
                    stdout: String::new(),
                    stderr: String::new(),
                    preview: None,
                });
        }
    }
}

#[allow(dead_code)] // bin 启动线程通过 runtime::execute_script_provider_test 间接调用
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
    state: &Rc<RefCell<AppState>>,
    config: ScriptProviderConfig,
    original_yaml_filename: Option<String>,
    original_script_filename: Option<String>,
    is_editing: bool,
) {
    let yaml_filename =
        original_yaml_filename.unwrap_or_else(|| api::generate_script_yaml_filename(&config));
    let script_filename =
        original_script_filename.unwrap_or_else(|| api::generate_script_filename(&config));

    match api::save_script_provider(&config, &yaml_filename, &script_filename, is_editing) {
        Ok((yaml_path, script_path)) => {
            info!(
                target: "runtime",
                "saved script provider YAML to {}, script to {}",
                yaml_path.display(),
                script_path.display()
            );
            let s = state.borrow();
            let settings_saved = s.settings_writer.flush(s.session.settings.clone());
            drop(s);
            let (title_key, body_key) = script_provider_ops::script_provider_save_notification_keys(
                is_editing,
                settings_saved,
            );
            crate::platform::notification::send_plain_notification(
                rust_i18n::t!(title_key).as_ref(),
                rust_i18n::t!(body_key).as_ref(),
            );
            let _ = super::refresh::send_request(state, RefreshRequest::ReloadProviders);
        }
        Err(err) => {
            warn!(target: "runtime", "failed to save script provider: {}", err);
            let mut s = state.borrow_mut();
            if is_editing {
                script_provider_ops::rollback_script_provider_edit(
                    &mut s.session,
                    &config,
                    &yaml_filename,
                    &script_filename,
                );
            } else {
                script_provider_ops::rollback_script_provider_create(&mut s.session, &config);
            }
            drop(s);
            let (title_key, body_key) =
                script_provider_ops::script_provider_save_failed_notification_keys();
            crate::platform::notification::send_plain_notification(
                rust_i18n::t!(title_key).as_ref(),
                rust_i18n::t!(body_key).as_ref(),
            );
        }
    }
}

fn delete_provider(state: &Rc<RefCell<AppState>>, provider_id: ProviderId) {
    match api::delete_script_provider_files(&provider_id) {
        Ok((yaml_path, script_result)) => {
            match script_result {
                Ok(script_path) => {
                    info!(
                        target: "runtime",
                        "deleted script provider files: {}, {}",
                        yaml_path.display(),
                        script_path.display()
                    );
                }
                Err(err) => {
                    warn!(
                        target: "runtime",
                        "deleted script provider YAML {}, but failed to delete companion script: {}",
                        yaml_path.display(),
                        err
                    );
                    crate::platform::notification::send_plain_notification(
                        rust_i18n::t!("script_provider.delete_partial_title").as_ref(),
                        rust_i18n::t!("script_provider.delete_partial_body").as_ref(),
                    );
                }
            }
            let _ = super::refresh::send_request(state, RefreshRequest::ReloadProviders);
        }
        Err(err) => {
            warn!(target: "runtime", "{err}");
            crate::platform::notification::send_plain_notification(
                rust_i18n::t!("script_provider.delete_failed_title").as_ref(),
                rust_i18n::t!("script_provider.delete_failed_body").as_ref(),
            );
        }
    }
}

fn load_config(state: &Rc<RefCell<AppState>>, provider_id: ProviderId) {
    if let ProviderId::Custom(ref custom_id) = provider_id {
        if let Some(edit_data) = api::read_script_provider_config(custom_id) {
            let mut s = state.borrow_mut();
            s.session.settings_ui.modal =
                crate::application::SettingsModalState::EditingScriptProvider(edit_data);
            s.session.settings_ui.script_provider_test_result = None;
        } else {
            warn!(
                target: "settings",
                "ScriptProviderEffect::LoadConfig: failed to read config for {}",
                custom_id
            );
            crate::platform::notification::send_plain_notification(
                rust_i18n::t!("script_provider.load_failed_title").as_ref(),
                rust_i18n::t!("script_provider.load_failed_body").as_ref(),
            );
        }
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
