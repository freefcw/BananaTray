use super::common::{has_effect, has_render, make_custom_provider_status, make_session};
use crate::application::{
    reduce, AppAction, AppEffect, CommonEffect, ScriptProviderEffect, SettingsModalState,
};
use crate::models::{ProviderId, ScriptProviderConfig, ScriptProviderTestResult};

fn make_script_config() -> ScriptProviderConfig {
    ScriptProviderConfig {
        display_name: "ccswitch".to_string(),
        provider_id: "ccswitch:script".to_string(),
        interpreter: "python3".to_string(),
        timeout_ms: 20_000,
        script: "print('{\"remaining\": 1}')".to_string(),
    }
}

#[test]
fn enter_add_script_provider_sets_exclusive_flag() {
    let mut session = make_session();
    session.settings_ui.modal = SettingsModalState::AddingNewApi;

    let effects = reduce(&mut session, AppAction::EnterAddScriptProvider);

    assert_eq!(
        session.settings_ui.modal,
        SettingsModalState::AddingScriptProvider
    );
    assert!(has_render(&effects));
}

#[test]
fn enter_add_script_provider_clears_confirming_flags() {
    let mut session = make_session();
    session.settings_ui.modal = SettingsModalState::ConfirmingDeleteScriptProvider;

    let effects = reduce(&mut session, AppAction::EnterAddScriptProvider);

    assert_eq!(
        session.settings_ui.modal,
        SettingsModalState::AddingScriptProvider
    );
    assert!(has_render(&effects));
}

#[test]
fn submit_script_provider_auto_enables_and_emits_save_effect() {
    let mut session = make_session();
    session.settings_ui.modal = SettingsModalState::AddingScriptProvider;
    let config = make_script_config();

    let effects = reduce(
        &mut session,
        AppAction::SubmitScriptProvider(config.clone()),
    );

    let expected_id = ProviderId::Custom("ccswitch:script".to_string());
    assert!(session.settings.provider.is_enabled(&expected_id));
    assert!(session
        .settings
        .provider
        .sidebar_providers
        .contains(&"ccswitch:script".to_string()));
    assert_eq!(session.settings_ui.selected_provider, expected_id);
    assert!(!session.settings_ui.modal.is_script_provider_form());
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::ScriptProvider(ScriptProviderEffect::SaveProvider { config: effect_config, is_editing, .. }))
            if effect_config.provider_id == config.provider_id && !is_editing
    )));
}

#[test]
fn submit_script_provider_assigns_unique_id_when_slug_exists() {
    let mut session = make_session();
    session.settings_ui.modal = SettingsModalState::AddingScriptProvider;
    session
        .provider_store
        .providers
        .push(make_custom_provider_status("custom-script:script"));
    session.settings.provider.set_enabled(
        &ProviderId::Custom("custom-script-2:script".to_string()),
        false,
    );
    let config = ScriptProviderConfig {
        display_name: "月之暗面".to_string(),
        provider_id: "custom-script:script".to_string(),
        ..make_script_config()
    };

    let effects = reduce(
        &mut session,
        AppAction::SubmitScriptProvider(config.clone()),
    );

    let expected_id = ProviderId::Custom("custom-script-3:script".to_string());
    assert_eq!(session.settings_ui.selected_provider, expected_id);
    assert!(session.settings.provider.is_enabled(&expected_id));
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::ScriptProvider(ScriptProviderEffect::SaveProvider { config: effect_config, is_editing, .. }))
            if effect_config.provider_id == "custom-script-3:script" && !is_editing
    )));
}

#[test]
fn submit_script_provider_edit_mode_does_not_duplicate_sidebar_entry() {
    use crate::models::ScriptProviderEditData;

    let mut session = make_session();
    session.settings_ui.modal = SettingsModalState::EditingScriptProvider(ScriptProviderEditData {
        display_name: "Script".to_string(),
        provider_id: "script:script".to_string(),
        interpreter: "python3".to_string(),
        timeout_ms: 20_000,
        script: "print(1)".to_string(),
        original_yaml_filename: "script-script.yaml".to_string(),
        original_script_filename: "script-script.py".to_string(),
    });
    session
        .settings
        .provider
        .sidebar_providers
        .push("script:script".to_string());

    reduce(
        &mut session,
        AppAction::SubmitScriptProvider(make_script_config()),
    );

    assert_eq!(
        session
            .settings
            .provider
            .sidebar_providers
            .iter()
            .filter(|id| *id == "script:script")
            .count(),
        1
    );
}

#[test]
fn test_script_provider_emits_test_effect_and_clears_old_result() {
    let mut session = make_session();
    session.settings_ui.script_provider_test_result = Some(ScriptProviderTestResult {
        success: true,
        message: "old".to_string(),
        stdout: "{}".to_string(),
        stderr: String::new(),
        preview: None,
    });
    let config = make_script_config();

    let effects = reduce(&mut session, AppAction::TestScriptProvider(config.clone()));

    assert!(session.settings_ui.script_provider_test_result.is_none());
    assert!(session.settings_ui.script_provider_testing);
    assert_eq!(
        session.settings_ui.script_provider_pending_test_request_id,
        Some(1)
    );
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::ScriptProvider(ScriptProviderEffect::TestProvider { request_id, config: effect_config }))
            if *request_id == 1 && effect_config.provider_id == config.provider_id
    )));
}

#[test]
fn script_provider_test_finished_stores_matching_result() {
    let mut session = make_session();
    session.settings_ui.script_provider_testing = true;
    session.settings_ui.script_provider_pending_test_request_id = Some(7);
    let result = ScriptProviderTestResult {
        success: true,
        message: "ok".to_string(),
        stdout: "{}".to_string(),
        stderr: String::new(),
        preview: None,
    };

    let effects = reduce(
        &mut session,
        AppAction::ScriptProviderTestFinished {
            request_id: 7,
            result: result.clone(),
        },
    );

    assert!(!session.settings_ui.script_provider_testing);
    assert!(session
        .settings_ui
        .script_provider_pending_test_request_id
        .is_none());
    assert_eq!(
        session.settings_ui.script_provider_test_result,
        Some(result)
    );
    assert!(has_render(&effects));
}

#[test]
fn script_provider_test_finished_ignores_stale_result() {
    let mut session = make_session();
    session.settings_ui.script_provider_testing = true;
    session.settings_ui.script_provider_pending_test_request_id = Some(8);
    let result = ScriptProviderTestResult {
        success: true,
        message: "old".to_string(),
        stdout: "{}".to_string(),
        stderr: String::new(),
        preview: None,
    };

    let effects = reduce(
        &mut session,
        AppAction::ScriptProviderTestFinished {
            request_id: 7,
            result,
        },
    );

    assert!(session.settings_ui.script_provider_testing);
    assert_eq!(
        session.settings_ui.script_provider_pending_test_request_id,
        Some(8)
    );
    assert!(session.settings_ui.script_provider_test_result.is_none());
    assert!(effects.is_empty());
}

#[test]
fn edit_script_provider_emits_load_config_effect() {
    let mut session = make_session();
    let id = ProviderId::Custom("ccswitch:script".to_string());

    let effects = reduce(
        &mut session,
        AppAction::EditScriptProvider {
            provider_id: id.clone(),
        },
    );

    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::ScriptProvider(ScriptProviderEffect::LoadConfig { provider_id }))
            if *provider_id == id
    )));
    assert!(has_render(&effects));
}

#[test]
fn edit_script_provider_clears_stale_test_state() {
    let mut session = make_session();
    session.settings_ui.script_provider_testing = true;
    session.settings_ui.script_provider_pending_test_request_id = Some(42);
    session.settings_ui.script_provider_test_result = Some(ScriptProviderTestResult {
        success: true,
        message: String::new(),
        stdout: String::new(),
        stderr: String::new(),
        preview: None,
    });
    let id = ProviderId::Custom("ccswitch:script".to_string());

    let _effects = reduce(
        &mut session,
        AppAction::EditScriptProvider {
            provider_id: id.clone(),
        },
    );

    assert!(!session.settings_ui.script_provider_testing);
    assert_eq!(
        session.settings_ui.script_provider_pending_test_request_id,
        None
    );
    assert_eq!(session.settings_ui.script_provider_test_result, None);
}

#[test]
fn delete_script_provider_produces_delete_effect() {
    let mut session = make_session();
    let id = ProviderId::Custom("ccswitch:script".to_string());

    let effects = reduce(
        &mut session,
        AppAction::DeleteScriptProvider {
            provider_id: id.clone(),
        },
    );

    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::ScriptProvider(ScriptProviderEffect::DeleteProvider { provider_id }))
            if *provider_id == id
    )));
}
