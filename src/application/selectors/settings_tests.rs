#![allow(deprecated)]
use super::*;
use crate::application::FormIdentity;
use crate::models::test_helpers::{
    make_test_provider as make_provider, setup_test_locale as setup_locale,
};
use crate::models::{
    AppSettings, ConnectionStatus, FailureReason, ProviderFailure, ProviderKind, ProviderStatus,
    QuotaLabelSpec, SettingsCapability,
};

fn pid(kind: ProviderKind) -> ProviderId {
    ProviderId::BuiltIn(kind)
}

fn make_session(
    settings: AppSettings,
    selected_provider: ProviderId,
    providers: Vec<ProviderStatus>,
) -> AppSession {
    let mut session = AppSession::new(settings, providers);
    session.settings_ui.selected_provider = selected_provider;
    session
}

fn test_failure(message: &str) -> ProviderFailure {
    ProviderFailure {
        reason: FailureReason::FetchFailed,
        advice: None,
        raw_detail: Some(message.to_string()),
    }
}

fn detail_snapshot(session: &AppSession) -> SettingsProviderDetailViewState {
    settings_providers_tab_view_state(session).detail
}

fn assert_detail_confirmation_flags(
    session: &AppSession,
    remove: bool,
    delete_newapi: bool,
    delete_script_provider: bool,
) {
    let detail = detail_snapshot(session);
    assert_eq!(detail.confirming_remove, remove);
    assert_eq!(detail.confirming_delete_newapi, delete_newapi);
    assert_eq!(
        detail.confirming_delete_script_provider,
        delete_script_provider
    );
}

fn detail_quota_visible(session: &AppSession, quota_key: &str) -> bool {
    detail_snapshot(session)
        .quota_visibility
        .iter()
        .find(|item| item.quota_key == quota_key)
        .unwrap_or_else(|| panic!("missing quota visibility item for key {quota_key}"))
        .visible
}

#[test]
fn settings_providers_tab_respects_order_and_selection() {
    let _locale_guard = setup_locale();
    let mut settings = AppSettings {
        provider: crate::models::ProviderConfig {
            provider_order: vec!["gemini".into(), "claude".into(), "copilot".into()],
            sidebar_providers: vec!["gemini".into(), "claude".into(), "copilot".into()],
            ..Default::default()
        },
        ..Default::default()
    };
    settings
        .provider
        .set_provider_enabled(ProviderKind::Gemini, true);
    settings
        .provider
        .set_provider_enabled(ProviderKind::Claude, true);
    settings
        .provider
        .set_provider_enabled(ProviderKind::Copilot, true);

    let session = make_session(
        settings,
        pid(ProviderKind::Claude),
        vec![
            make_provider(ProviderKind::Gemini, ConnectionStatus::Connected),
            make_provider(ProviderKind::Claude, ConnectionStatus::Connected),
            make_provider(ProviderKind::Copilot, ConnectionStatus::Connected),
        ],
    );

    let view_state = settings_providers_tab_view_state(&session);

    assert_eq!(view_state.items[0].id, pid(ProviderKind::Gemini));
    assert!(!view_state.items[0].is_selected);
    assert_eq!(view_state.items[1].id, pid(ProviderKind::Claude));
    assert!(view_state.items[1].is_selected);
    assert_eq!(view_state.items[2].id, pid(ProviderKind::Copilot));
    assert!(!view_state.items[2].is_selected);
}

#[test]
fn settings_providers_tab_right_pane_defaults_to_detail() {
    let _locale_guard = setup_locale();
    let settings = AppSettings::default();
    let provider = make_provider(ProviderKind::Claude, ConnectionStatus::Connected);
    let session = make_session(settings, pid(ProviderKind::Claude), vec![provider]);

    let view_state = settings_providers_tab_view_state(&session);

    assert_eq!(
        view_state.right_pane,
        SettingsProviderRightPaneViewState::Detail
    );
}

#[test]
fn settings_providers_tab_right_pane_reports_provider_picker() {
    let _locale_guard = setup_locale();
    let settings = AppSettings::default();
    let provider = make_provider(ProviderKind::Claude, ConnectionStatus::Connected);
    let mut session = make_session(settings, pid(ProviderKind::Claude), vec![provider]);
    session.settings_ui.modal = SettingsModalState::AddingProvider;

    let view_state = settings_providers_tab_view_state(&session);

    assert_eq!(
        view_state.right_pane,
        SettingsProviderRightPaneViewState::ProviderPicker
    );
}

#[test]
fn settings_providers_tab_right_pane_reports_newapi_form() {
    use crate::models::NewApiEditData;

    let _locale_guard = setup_locale();
    let settings = AppSettings::default();
    let provider = make_provider(ProviderKind::Claude, ConnectionStatus::Connected);
    let mut session = make_session(settings, pid(ProviderKind::Claude), vec![provider]);
    let edit_data = NewApiEditData {
        display_name: "Relay".to_string(),
        base_url: "https://relay.example.com".to_string(),
        cookie: "c=1".to_string(),
        user_id: None,
        divisor: None,
        original_filename: "relay.yaml".to_string(),
        original_id: "relay-example-com:newapi".to_string(),
    };
    session.settings_ui.modal = SettingsModalState::EditingNewApi(edit_data.clone());

    let view_state = settings_providers_tab_view_state(&session);

    assert_eq!(
        view_state.right_pane,
        SettingsProviderRightPaneViewState::NewApiForm {
            identity: FormIdentity::NewApiEdit {
                original_filename: "relay.yaml".into()
            },
            edit_data: Some(edit_data)
        }
    );
}

#[test]
fn settings_providers_tab_right_pane_reports_add_form_identity() {
    let _locale_guard = setup_locale();
    let settings = AppSettings::default();
    let provider = make_provider(ProviderKind::Claude, ConnectionStatus::Connected);
    let mut session = make_session(settings, pid(ProviderKind::Claude), vec![provider]);
    session.settings_ui.modal = SettingsModalState::AddingNewApi;

    let view_state = settings_providers_tab_view_state(&session);

    assert_eq!(
        view_state.right_pane,
        SettingsProviderRightPaneViewState::NewApiForm {
            identity: FormIdentity::NewApiAdd,
            edit_data: None,
        }
    );
}

#[test]
fn settings_providers_tab_right_pane_reports_script_form_identity() {
    use crate::models::ScriptProviderEditData;

    let _locale_guard = setup_locale();
    let settings = AppSettings::default();
    let provider = make_provider(ProviderKind::Claude, ConnectionStatus::Connected);
    let mut session = make_session(settings, pid(ProviderKind::Claude), vec![provider]);
    let edit_data = ScriptProviderEditData {
        display_name: "Script".to_string(),
        provider_id: "script:script".to_string(),
        interpreter: "python3".to_string(),
        timeout_ms: 12_000,
        script: "print(1)".to_string(),
        original_yaml_filename: "script.yaml".to_string(),
        original_script_filename: "script.py".to_string(),
    };
    session.settings_ui.modal = SettingsModalState::EditingScriptProvider(edit_data.clone());

    let view_state = settings_providers_tab_view_state(&session);

    assert_eq!(
        view_state.right_pane,
        SettingsProviderRightPaneViewState::ScriptProviderForm {
            identity: FormIdentity::ScriptProviderEdit {
                original_yaml_filename: "script.yaml".into(),
                original_script_filename: "script.py".into(),
            },
            edit_data: Some(edit_data),
            testing: false,
            test_result: None,
        }
    );
}

#[test]
fn settings_provider_detail_reports_disabled_usage() {
    let _locale_guard = setup_locale();
    let mut settings = AppSettings::default();
    settings
        .provider
        .set_provider_enabled(ProviderKind::Claude, false);

    let session = make_session(
        settings,
        pid(ProviderKind::Claude),
        vec![make_provider(
            ProviderKind::Claude,
            ConnectionStatus::Disconnected,
        )],
    );

    let view_state = settings_providers_tab_view_state(&session);

    assert!(!view_state.detail.is_enabled);
    assert_eq!(view_state.detail.info.state_text, "Disabled");
    assert!(matches!(
        view_state.detail.usage,
        SettingsProviderUsageViewState::Disabled { .. }
    ));
}

#[test]
fn settings_provider_detail_reports_confirming_actions() {
    let _locale_guard = setup_locale();
    let settings = AppSettings::default();
    let provider = make_provider(ProviderKind::Claude, ConnectionStatus::Connected);
    let mut session = make_session(settings, pid(ProviderKind::Claude), vec![provider]);

    session.settings_ui.modal = SettingsModalState::ConfirmingRemoveProvider;
    assert_detail_confirmation_flags(&session, true, false, false);

    session.settings_ui.modal = SettingsModalState::ConfirmingDeleteNewApi;
    assert_detail_confirmation_flags(&session, false, true, false);

    session.settings_ui.modal = SettingsModalState::ConfirmingDeleteScriptProvider;
    assert_detail_confirmation_flags(&session, false, false, true);
}

#[test]
fn settings_provider_detail_confirmation_flags_track_reducer_actions() {
    let _locale_guard = setup_locale();
    let settings = AppSettings::default();
    let provider = make_provider(ProviderKind::Claude, ConnectionStatus::Connected);
    let mut session = make_session(settings, pid(ProviderKind::Claude), vec![provider]);

    assert_detail_confirmation_flags(&session, false, false, false);

    crate::application::reduce(
        &mut session,
        crate::application::AppAction::ConfirmRemoveProvider,
    );
    assert_detail_confirmation_flags(&session, true, false, false);

    crate::application::reduce(
        &mut session,
        crate::application::AppAction::CancelRemoveProvider,
    );
    assert_detail_confirmation_flags(&session, false, false, false);

    crate::application::reduce(
        &mut session,
        crate::application::AppAction::ConfirmDeleteNewApi,
    );
    assert_detail_confirmation_flags(&session, false, true, false);

    crate::application::reduce(
        &mut session,
        crate::application::AppAction::CancelDeleteNewApi,
    );
    assert_detail_confirmation_flags(&session, false, false, false);

    crate::application::reduce(
        &mut session,
        crate::application::AppAction::ConfirmDeleteScriptProvider,
    );
    assert_detail_confirmation_flags(&session, false, false, true);

    crate::application::reduce(
        &mut session,
        crate::application::AppAction::CancelDeleteScriptProvider,
    );
    assert_detail_confirmation_flags(&session, false, false, false);
}

#[test]
fn settings_provider_subtitle_uses_display_source_label() {
    let _locale_guard = setup_locale();
    let mut settings = AppSettings::default();
    settings
        .provider
        .set_provider_enabled(ProviderKind::Copilot, true);

    let mut provider = make_provider(ProviderKind::Copilot, ConnectionStatus::Connected);
    provider.runtime_source_label = Some("github api".to_string());
    provider.last_refreshed_instant = Some(std::time::Instant::now());

    let session = make_session(settings, pid(ProviderKind::Copilot), vec![provider]);
    let view_state = settings_providers_tab_view_state(&session);

    assert!(
        view_state.detail.subtitle.starts_with("GitHub · just now"),
        "subtitle should use compact time, got: {}",
        view_state.detail.subtitle
    );
    assert!(!view_state.detail.subtitle.contains("github api"));
}

#[test]
fn settings_provider_detail_reports_error_usage() {
    let _locale_guard = setup_locale();
    let mut settings = AppSettings::default();
    settings
        .provider
        .set_provider_enabled(ProviderKind::Copilot, true);

    let mut provider = make_provider(ProviderKind::Copilot, ConnectionStatus::Error);
    provider.last_failure = Some(test_failure("boom"));

    let session = make_session(settings, pid(ProviderKind::Copilot), vec![provider]);
    let view_state = settings_providers_tab_view_state(&session);

    assert!(matches!(
        view_state.detail.settings_capability,
        SettingsCapability::TokenInput(_)
    ));
    assert_eq!(
        view_state.detail.info.status_kind,
        SettingsProviderStatusKind::Error
    );
    assert!(matches!(
        view_state.detail.usage,
        SettingsProviderUsageViewState::Error { .. }
    ));
}

#[test]
fn settings_provider_detail_marks_non_monitorable_provider() {
    let _locale_guard = setup_locale();
    let mut settings = AppSettings::default();
    settings
        .provider
        .set_provider_enabled(ProviderKind::Kilo, true);

    let provider = make_provider(ProviderKind::Kilo, ConnectionStatus::Disconnected);
    let session = make_session(settings, pid(ProviderKind::Kilo), vec![provider]);
    let view_state = settings_providers_tab_view_state(&session);

    assert!(!view_state.detail.can_refresh);
    assert!(!view_state.detail.show_quota_visibility);
    assert_eq!(view_state.detail.info.source_text, "Reference");
    assert_eq!(view_state.detail.info.updated_text, "Not applicable");
    assert_eq!(view_state.detail.info.status_text, "Not monitorable");
    assert!(matches!(
        view_state.detail.usage,
        SettingsProviderUsageViewState::Empty { .. }
    ));
}

// ── quota_visibility 构建 ────────────────────────────

#[test]
fn settings_detail_builds_quota_visibility_from_stable_key() {
    use crate::models::{QuotaInfo, QuotaType};

    let _locale_guard = setup_locale();
    let mut settings = AppSettings::default();
    settings
        .provider
        .set_provider_enabled(ProviderKind::Claude, true);
    // 隐藏 session 类型配额
    settings
        .provider
        .toggle_quota_visibility(&pid(ProviderKind::Claude), "session".to_string());

    let mut provider = make_provider(ProviderKind::Claude, ConnectionStatus::Connected);
    provider.quotas = vec![
        QuotaInfo::with_details(
            QuotaLabelSpec::Session,
            30.0,
            100.0,
            QuotaType::Session,
            None,
        ),
        QuotaInfo::with_details(QuotaLabelSpec::Weekly, 50.0, 100.0, QuotaType::Weekly, None),
    ];

    let session = make_session(settings, pid(ProviderKind::Claude), vec![provider]);
    let view_state = settings_providers_tab_view_state(&session);

    assert_eq!(view_state.detail.quota_visibility.len(), 2);
    // Session 应被标记为不可见（使用 stable_key 匹配，不依赖 label）
    assert_eq!(view_state.detail.quota_visibility[0].quota_key, "session");
    assert!(!view_state.detail.quota_visibility[0].visible);
    // Weekly 应仍可见
    assert_eq!(view_state.detail.quota_visibility[1].quota_key, "weekly");
    assert!(view_state.detail.quota_visibility[1].visible);
}

#[test]
fn settings_detail_quota_visibility_tracks_toggle_action() {
    use crate::models::{QuotaInfo, QuotaType};

    let _locale_guard = setup_locale();
    let provider_id = pid(ProviderKind::Claude);
    let mut settings = AppSettings::default();
    settings
        .provider
        .set_provider_enabled(ProviderKind::Claude, true);

    let mut provider = make_provider(ProviderKind::Claude, ConnectionStatus::Connected);
    provider.quotas = vec![
        QuotaInfo::with_details(
            QuotaLabelSpec::Session,
            30.0,
            100.0,
            QuotaType::Session,
            None,
        ),
        QuotaInfo::with_details(QuotaLabelSpec::Weekly, 50.0, 100.0, QuotaType::Weekly, None),
    ];

    let mut session = make_session(settings, provider_id.clone(), vec![provider]);
    assert!(detail_quota_visible(&session, "session"));

    crate::application::reduce(
        &mut session,
        crate::application::AppAction::UpdateSetting(
            crate::application::SettingChange::ToggleQuotaVisibility {
                provider_id: provider_id.clone(),
                quota_key: "session".to_string(),
            },
        ),
    );
    assert!(!detail_quota_visible(&session, "session"));

    crate::application::reduce(
        &mut session,
        crate::application::AppAction::UpdateSetting(
            crate::application::SettingChange::ToggleQuotaVisibility {
                provider_id,
                quota_key: "session".to_string(),
            },
        ),
    );
    assert!(detail_quota_visible(&session, "session"));
}

#[test]
fn settings_detail_quota_visibility_empty_when_no_quotas() {
    let _locale_guard = setup_locale();
    let mut settings = AppSettings::default();
    settings
        .provider
        .set_provider_enabled(ProviderKind::Claude, true);

    let provider = make_provider(ProviderKind::Claude, ConnectionStatus::Connected);
    let session = make_session(settings, pid(ProviderKind::Claude), vec![provider]);
    let view_state = settings_providers_tab_view_state(&session);

    assert!(view_state.detail.quota_visibility.is_empty());
}

// ── QuotaDisplayMode 透传 ────────────────────────────

#[test]
fn settings_detail_inherits_quota_display_mode() {
    use crate::models::QuotaDisplayMode;

    let _locale_guard = setup_locale();
    let mut settings = AppSettings::default();
    settings
        .provider
        .set_provider_enabled(ProviderKind::Claude, true);
    settings.display.quota_display_mode = QuotaDisplayMode::Used;

    let session = make_session(
        settings,
        pid(ProviderKind::Claude),
        vec![make_provider(
            ProviderKind::Claude,
            ConnectionStatus::Connected,
        )],
    );

    let view_state = settings_providers_tab_view_state(&session);
    assert_eq!(view_state.detail.quota_display_mode, QuotaDisplayMode::Used);
}

// ── settings_capability 透传 ────────────────────────────

#[test]
fn settings_capability_none_for_plain_builtin() {
    let _locale_guard = setup_locale();
    let settings = AppSettings::default();
    let provider = make_provider(ProviderKind::Claude, ConnectionStatus::Connected);
    let session = make_session(settings, pid(ProviderKind::Claude), vec![provider]);
    let view_state = settings_providers_tab_view_state(&session);
    assert_eq!(
        view_state.detail.settings_capability,
        SettingsCapability::None
    );
}

#[test]
fn settings_capability_token_input_for_copilot() {
    let _locale_guard = setup_locale();
    let settings = AppSettings::default();
    let provider = make_provider(ProviderKind::Copilot, ConnectionStatus::Connected);
    let session = make_session(settings, pid(ProviderKind::Copilot), vec![provider]);
    let view_state = settings_providers_tab_view_state(&session);
    assert!(matches!(
        view_state.detail.settings_capability,
        SettingsCapability::TokenInput(_)
    ));
}

#[test]
fn settings_capability_newapi_editable_for_custom_newapi() {
    let _locale_guard = setup_locale();
    let settings = AppSettings::default();
    let id = ProviderId::Custom("my-site:newapi".to_string());
    let metadata = crate::models::test_helpers::make_test_metadata(ProviderKind::Custom);
    let mut provider = ProviderStatus::new(id.clone(), metadata);
    provider.settings_capability = SettingsCapability::NewApiEditable {
        base_url: "https://my-site.com".to_string(),
    };
    let session = make_session(settings, id, vec![provider]);
    let view_state = settings_providers_tab_view_state(&session);
    // 站点地址要原样带到详情视图，设置卡片靠它标明这张卡管的是哪个中转站
    assert_eq!(
        view_state.detail.settings_capability,
        SettingsCapability::NewApiEditable {
            base_url: "https://my-site.com".to_string()
        }
    );
}

#[test]
fn settings_capability_script_editable_for_custom_script() {
    let _locale_guard = setup_locale();
    let settings = AppSettings::default();
    let id = ProviderId::Custom("my-script:script".to_string());
    let metadata = crate::models::test_helpers::make_test_metadata(ProviderKind::Custom);
    let mut provider = ProviderStatus::new(id.clone(), metadata);
    provider.settings_capability = SettingsCapability::ScriptEditable {
        interpreter: "python3".to_string(),
    };
    let session = make_session(settings, id, vec![provider]);
    let view_state = settings_providers_tab_view_state(&session);
    assert_eq!(
        view_state.detail.settings_capability,
        SettingsCapability::ScriptEditable {
            interpreter: "python3".to_string()
        }
    );
}

#[test]
fn settings_capability_defaults_when_provider_missing() {
    let _locale_guard = setup_locale();
    let settings = AppSettings::default();
    // 没有 provider 数据，capability 应为默认值 None
    let session = make_session(settings, pid(ProviderKind::Claude), vec![]);
    let view_state = settings_providers_tab_view_state(&session);
    assert_eq!(
        view_state.detail.settings_capability,
        SettingsCapability::None
    );
}
