use super::*;
use crate::models::test_helpers::{
    make_test_provider as make_provider, setup_test_locale as setup_locale,
};
use crate::models::{
    AppSettings, ConnectionStatus, ProviderKind, ProviderStatus, SettingsCapability,
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

#[test]
fn settings_providers_tab_respects_order_and_selection() {
    let _locale_guard = setup_locale();
    let mut settings = AppSettings {
        provider: crate::models::ProviderConfig {
            provider_order: vec!["gemini".into(), "claude".into(), "copilot".into()],
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
fn settings_provider_detail_reports_error_usage() {
    let _locale_guard = setup_locale();
    let mut settings = AppSettings::default();
    settings
        .provider
        .set_provider_enabled(ProviderKind::Copilot, true);

    let mut provider = make_provider(ProviderKind::Copilot, ConnectionStatus::Error);
    provider.error_message = Some("boom".to_string());

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
        .toggle_quota_visibility(ProviderKind::Claude, "session".to_string());

    let mut provider = make_provider(ProviderKind::Claude, ConnectionStatus::Connected);
    provider.quotas = vec![
        QuotaInfo::with_details(
            String::from("Session (5h)"),
            30.0,
            100.0,
            QuotaType::Session,
            None,
        ),
        QuotaInfo::with_details(String::from("Weekly"), 50.0, 100.0, QuotaType::Weekly, None),
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
    provider.settings_capability = SettingsCapability::NewApiEditable;
    let session = make_session(settings, id, vec![provider]);
    let view_state = settings_providers_tab_view_state(&session);
    assert_eq!(
        view_state.detail.settings_capability,
        SettingsCapability::NewApiEditable
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
