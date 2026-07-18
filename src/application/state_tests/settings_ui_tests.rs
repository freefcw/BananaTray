use super::super::*;
use super::common::*;
use crate::models::{NewApiEditData, ProviderKind, ScriptProviderEditData};

#[test]
fn settings_ui_default_values() {
    let ui = SettingsUiState {
        active_tab: SettingsTab::General,
        selected_provider: pid(ProviderKind::Claude),
        cadence_dropdown_open: false,
        token_editing_provider: None,
        modal: SettingsModalState::Idle,
        script_provider_testing: false,
        script_provider_test_request_id: 0,
        script_provider_pending_test_request_id: None,
        script_provider_test_result: None,
        global_hotkey_error: None,
        global_hotkey_error_candidate: None,
    };
    assert_eq!(ui.active_tab, SettingsTab::General);
    assert!(!ui.cadence_dropdown_open);
    assert!(!ui.modal.is_script_provider_form());
    assert!(ui.modal.script_provider_edit_data().is_none());
    assert!(!ui.script_provider_testing);
    assert_eq!(ui.script_provider_test_request_id, 0);
    assert!(ui.script_provider_pending_test_request_id.is_none());
    assert!(ui.script_provider_test_result.is_none());
    assert!(ui.global_hotkey_error.is_none());
    assert!(ui.global_hotkey_error_candidate.is_none());
}

#[test]
fn clear_script_provider_transient_state_preserves_unrelated_settings_ui_state() {
    let selected_provider = pid(ProviderKind::Claude);
    let token_editing_provider = pid(ProviderKind::Copilot);
    let mut ui = SettingsUiState {
        active_tab: SettingsTab::Providers,
        selected_provider: selected_provider.clone(),
        cadence_dropdown_open: true,
        token_editing_provider: Some(token_editing_provider.clone()),
        modal: SettingsModalState::AddingScriptProvider,
        script_provider_testing: true,
        script_provider_test_request_id: 17,
        script_provider_pending_test_request_id: Some(17),
        script_provider_test_result: Some(ScriptProviderTestResult {
            success: true,
            message: "ok".into(),
            stdout: "stdout".into(),
            stderr: String::new(),
            preview: None,
        }),
        global_hotkey_error: Some(GlobalHotkeyError::InvalidFormat),
        global_hotkey_error_candidate: Some("bad-hotkey".into()),
    };

    ui.clear_script_provider_transient_state();

    assert!(!ui.script_provider_testing);
    assert!(ui.script_provider_pending_test_request_id.is_none());
    assert!(ui.script_provider_test_result.is_none());
    assert_eq!(ui.script_provider_test_request_id, 17);
    assert_eq!(ui.active_tab, SettingsTab::Providers);
    assert_eq!(ui.selected_provider, selected_provider);
    assert!(ui.cadence_dropdown_open);
    assert_eq!(ui.token_editing_provider, Some(token_editing_provider));
    assert_eq!(ui.modal, SettingsModalState::AddingScriptProvider);
    assert_eq!(
        ui.global_hotkey_error,
        Some(GlobalHotkeyError::InvalidFormat)
    );
    assert_eq!(
        ui.global_hotkey_error_candidate.as_deref(),
        Some("bad-hotkey")
    );
}

#[test]
fn global_hotkey_error_helpers_preserve_error_candidate_pair_invariant() {
    let mut ui = SettingsUiState {
        active_tab: SettingsTab::General,
        selected_provider: pid(ProviderKind::Claude),
        cadence_dropdown_open: false,
        token_editing_provider: None,
        modal: SettingsModalState::Idle,
        script_provider_testing: false,
        script_provider_test_request_id: 0,
        script_provider_pending_test_request_id: None,
        script_provider_test_result: None,
        global_hotkey_error: None,
        global_hotkey_error_candidate: None,
    };

    ui.record_global_hotkey_error(
        "cmd-shift-j".into(),
        GlobalHotkeyError::Conflict("already registered".into()),
    );

    assert_eq!(
        ui.global_hotkey_error,
        Some(GlobalHotkeyError::Conflict("already registered".into()))
    );
    assert_eq!(
        ui.global_hotkey_error_candidate.as_deref(),
        Some("cmd-shift-j")
    );

    ui.clear_global_hotkey_error();

    assert!(ui.global_hotkey_error.is_none());
    assert!(ui.global_hotkey_error_candidate.is_none());
}

#[test]
fn modal_state_helpers_match_variants() {
    let idle = SettingsModalState::Idle;
    assert!(!idle.is_newapi_form());
    assert!(!idle.is_adding_provider());
    assert!(!idle.is_confirming_remove_provider());
    assert!(!idle.is_confirming_delete_newapi());
    assert!(idle.newapi_edit_data().is_none());

    assert!(SettingsModalState::AddingNewApi.is_newapi_form());
    assert!(SettingsModalState::AddingNewApi
        .newapi_edit_data()
        .is_none());

    let edit = SettingsModalState::EditingNewApi(NewApiEditData {
        display_name: "x".into(),
        base_url: "https://example.com".into(),
        cookie: String::new(),
        user_id: None,
        divisor: None,
        original_filename: "x.yaml".into(),
    });
    assert!(edit.is_newapi_form());
    assert_eq!(edit.newapi_edit_data().unwrap().display_name, "x");

    assert!(SettingsModalState::AddingProvider.is_adding_provider());
    assert!(SettingsModalState::ConfirmingRemoveProvider.is_confirming_remove_provider());
    assert!(SettingsModalState::ConfirmingDeleteNewApi.is_confirming_delete_newapi());
    assert_eq!(
        SettingsModalState::AddingNewApi.form_identity(),
        Some(FormIdentity::NewApiAdd)
    );
    assert_eq!(
        edit.form_identity(),
        Some(FormIdentity::NewApiEdit {
            original_filename: "x.yaml".into()
        })
    );

    let script_edit = SettingsModalState::EditingScriptProvider(ScriptProviderEditData {
        display_name: "script".into(),
        provider_id: "script:script".into(),
        interpreter: "python3".into(),
        timeout_ms: 20_000,
        script: "print(1)".into(),
        original_yaml_filename: "script.yaml".into(),
        original_script_filename: "script.py".into(),
    });
    assert_eq!(
        script_edit.form_identity(),
        Some(FormIdentity::ScriptProviderEdit {
            original_yaml_filename: "script.yaml".into(),
            original_script_filename: "script.py".into(),
        })
    );
}

#[test]
fn debug_ui_default_values() {
    let debug = DebugUiState::default();
    assert!(debug.selected_provider.is_none());
    assert!(!debug.refresh_active);
    assert!(debug.prev_log_level.is_none());
}
