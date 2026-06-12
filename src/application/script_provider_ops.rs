//! Pure state helpers for script-provider save failures and notifications.

use super::state::{AppSession, SettingsModalState};
use crate::models::{ProviderId, ScriptProviderConfig, ScriptProviderEditData};

pub fn rollback_script_provider_edit(
    session: &mut AppSession,
    config: &ScriptProviderConfig,
    yaml_filename: &str,
    script_filename: &str,
) {
    session.settings_ui.modal = SettingsModalState::EditingScriptProvider(ScriptProviderEditData {
        display_name: config.display_name.clone(),
        provider_id: config.provider_id.clone(),
        interpreter: config.interpreter.clone(),
        timeout_ms: config.timeout_ms,
        script: config.script.clone(),
        original_yaml_filename: yaml_filename.to_string(),
        original_script_filename: script_filename.to_string(),
    });
}

pub fn rollback_script_provider_create(session: &mut AppSession, config: &ScriptProviderConfig) {
    let rollback_id = ProviderId::Custom(config.provider_id.clone());
    session
        .settings
        .provider
        .remove_enabled_record(&rollback_id);
    session.settings.provider.remove_from_sidebar(&rollback_id);

    session.settings_ui.modal = SettingsModalState::AddingScriptProvider;
    session.settings_ui.selected_provider = session.first_sidebar_provider();
}

pub fn script_provider_save_notification_keys(
    is_editing: bool,
    settings_saved: bool,
) -> (&'static str, &'static str) {
    if !settings_saved {
        (
            "script_provider.save_partial_title",
            "script_provider.save_partial_body",
        )
    } else if is_editing {
        (
            "script_provider.edit_success_title",
            "script_provider.edit_success_body",
        )
    } else {
        (
            "script_provider.save_success_title",
            "script_provider.save_success_body",
        )
    }
}

pub fn script_provider_save_failed_notification_keys() -> (&'static str, &'static str) {
    (
        "script_provider.save_failed_title",
        "script_provider.save_failed_body",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::AppSettings;

    fn make_session() -> AppSession {
        AppSession::new(AppSettings::default(), vec![])
    }

    fn make_config() -> ScriptProviderConfig {
        ScriptProviderConfig {
            display_name: "Script".to_string(),
            provider_id: "script:script".to_string(),
            interpreter: "python3".to_string(),
            timeout_ms: 20_000,
            script: "print('{}')".to_string(),
        }
    }

    #[test]
    fn rollback_create_removes_provider_registration() {
        let mut session = make_session();
        let config = make_config();
        let id = ProviderId::Custom(config.provider_id.clone());
        session.settings.provider.set_enabled(&id, true);
        session.settings.provider.add_to_sidebar(&id);

        rollback_script_provider_create(&mut session, &config);

        assert!(!session.settings.provider.is_enabled(&id));
        assert!(session.settings_ui.modal.is_script_provider_form());
    }

    #[test]
    fn rollback_edit_restores_form_data() {
        let mut session = make_session();
        let config = make_config();

        rollback_script_provider_edit(&mut session, &config, "provider.yaml", "script.py");

        assert_eq!(
            session.settings_ui.modal.script_provider_edit_data(),
            Some(&ScriptProviderEditData {
                display_name: "Script".to_string(),
                provider_id: "script:script".to_string(),
                interpreter: "python3".to_string(),
                timeout_ms: 20_000,
                script: "print('{}')".to_string(),
                original_yaml_filename: "provider.yaml".to_string(),
                original_script_filename: "script.py".to_string(),
            })
        );
    }

    #[test]
    fn notification_keys_save_failed() {
        let (title, body) = script_provider_save_failed_notification_keys();

        assert_eq!(title, "script_provider.save_failed_title");
        assert_eq!(body, "script_provider.save_failed_body");
    }

    #[test]
    fn notification_keys_save_success() {
        let (title, body) = script_provider_save_notification_keys(false, true);

        assert_eq!(title, "script_provider.save_success_title");
        assert_eq!(body, "script_provider.save_success_body");
    }

    #[test]
    fn notification_keys_edit_success() {
        let (title, body) = script_provider_save_notification_keys(true, true);

        assert_eq!(title, "script_provider.edit_success_title");
        assert_eq!(body, "script_provider.edit_success_body");
    }

    #[test]
    fn notification_keys_partial_when_settings_not_saved() {
        let (title, body) = script_provider_save_notification_keys(true, false);

        assert_eq!(title, "script_provider.save_partial_title");
        assert_eq!(body, "script_provider.save_partial_body");
    }
}
