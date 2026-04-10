use super::*;
use crate::models::test_helpers::{
    make_test_provider as make_provider, setup_test_locale as setup_locale,
};
use crate::models::{AppSettings, ConnectionStatus, ProviderKind, QuotaInfo};

fn pid(kind: ProviderKind) -> ProviderId {
    ProviderId::BuiltIn(kind)
}

fn make_session_with_provider(settings: AppSettings, provider: ProviderStatus) -> AppSession {
    let id = provider.provider_id.clone();
    let session = AppSession::new(settings, vec![provider]);
    assert_eq!(session.nav.active_tab, NavTab::Provider(id));
    session
}

// ── tray_global_actions_view_state ──────────────────────────

#[test]
fn global_actions_show_refresh_follows_setting() {
    let _locale_guard = setup_locale();
    let mut settings = AppSettings::default();
    settings
        .provider
        .set_provider_enabled(ProviderKind::Gemini, true);
    settings.display.show_refresh_button = false;

    let provider = make_provider(ProviderKind::Gemini, ConnectionStatus::Connected);
    let session = make_session_with_provider(settings, provider);
    let actions = tray_global_actions_view_state(&session);

    assert!(!actions.show_refresh);
}

#[test]
fn global_actions_refresh_id_matches_active_provider() {
    let _locale_guard = setup_locale();
    let mut settings = AppSettings::default();
    settings
        .provider
        .set_provider_enabled(ProviderKind::Gemini, true);

    let provider = make_provider(ProviderKind::Gemini, ConnectionStatus::Connected);
    let session = make_session_with_provider(settings, provider);
    let actions = tray_global_actions_view_state(&session);

    assert_eq!(actions.refresh.id, Some(pid(ProviderKind::Gemini)));
    assert!(!actions.refresh.is_refreshing);
}

#[test]
fn global_actions_is_refreshing_when_provider_refreshing() {
    let _locale_guard = setup_locale();
    let mut settings = AppSettings::default();
    settings
        .provider
        .set_provider_enabled(ProviderKind::Gemini, true);

    let provider = make_provider(ProviderKind::Gemini, ConnectionStatus::Refreshing);
    let session = make_session_with_provider(settings, provider);
    let actions = tray_global_actions_view_state(&session);

    assert!(actions.refresh.is_refreshing);
}

#[test]
fn global_actions_refresh_id_none_on_settings_tab() {
    let _locale_guard = setup_locale();
    let settings = AppSettings::default();
    // 没有任何 provider → active_tab 回退到 Settings
    let mut session = AppSession::new(settings, vec![]);
    session.nav.active_tab = NavTab::Settings;
    let actions = tray_global_actions_view_state(&session);

    assert!(actions.refresh.id.is_none());
}

// ── Account Info 冒烟测试 ─────────────────────────────────
// 边界组合（setting off / no email / dashboard off）已在
// application::state::tests::panel_flags_* 单元测试中覆盖，
// 这里只验证 selector 正确集成 flags → ViewModel 的端到端路径。

#[test]
fn account_card_assembled_when_email_and_setting_on() {
    let _locale_guard = setup_locale();
    let mut settings = AppSettings::default();
    settings
        .provider
        .set_provider_enabled(ProviderKind::Gemini, true);
    settings.display.show_account_info = true;
    settings.display.show_dashboard_button = true;

    let mut provider = make_provider(ProviderKind::Gemini, ConnectionStatus::Connected);
    provider.account_email = Some("test@example.com".to_string());
    provider.account_tier = Some("Pro".to_string());
    provider.quotas = vec![QuotaInfo::new("test", 50.0, 100.0)];

    let session = make_session_with_provider(settings, provider);
    let view = provider_detail_view_state(&session, &pid(ProviderKind::Gemini));

    match view {
        ProviderDetailViewState::Panel(panel) => {
            let account = panel.account.expect("account should be Some");
            assert_eq!(account.email, "test@example.com");
            assert_eq!(account.tier, Some("Pro".to_string()));
            assert!(account.dashboard_url.is_some());
            assert!(!panel.show_dashboard);
        }
        _ => panic!("expected Panel variant"),
    }
}

#[test]
fn no_account_card_when_email_absent() {
    let _locale_guard = setup_locale();
    let mut settings = AppSettings::default();
    settings
        .provider
        .set_provider_enabled(ProviderKind::Gemini, true);
    settings.display.show_account_info = true;
    settings.display.show_dashboard_button = true;

    let mut provider = make_provider(ProviderKind::Gemini, ConnectionStatus::Connected);
    provider.quotas = vec![QuotaInfo::new("test", 50.0, 100.0)];

    let session = make_session_with_provider(settings, provider);
    let view = provider_detail_view_state(&session, &pid(ProviderKind::Gemini));

    match view {
        ProviderDetailViewState::Panel(panel) => {
            assert!(panel.account.is_none());
            assert!(panel.show_dashboard);
        }
        _ => panic!("expected Panel variant"),
    }
}

// ── provider_detail_view_state 顶层分支 ──────────────────

#[test]
fn detail_returns_disabled_when_provider_is_disabled() {
    let _locale_guard = setup_locale();
    let settings = AppSettings::default(); // 默认所有 provider 未启用
    let provider = make_provider(ProviderKind::Gemini, ConnectionStatus::Connected);

    let session = AppSession::new(settings, vec![provider]);
    let view = provider_detail_view_state(&session, &pid(ProviderKind::Gemini));

    assert!(
        matches!(view, ProviderDetailViewState::Disabled(ref d) if d.id == pid(ProviderKind::Gemini)),
        "expected Disabled variant"
    );
}

#[test]
fn detail_prefers_disabled_over_missing_when_disabled_and_absent() {
    let _locale_guard = setup_locale();
    let settings = AppSettings::default();
    // 不注入任何 provider，但查询一个未启用的 id
    let session = AppSession::new(settings, vec![]);
    let view = provider_detail_view_state(&session, &pid(ProviderKind::Gemini));

    match view {
        ProviderDetailViewState::Disabled(d) => {
            assert_eq!(d.id, pid(ProviderKind::Gemini));
            assert!(
                d.icon.contains("unknown"),
                "absent provider should use unknown icon"
            );
        }
        _ => panic!("expected Disabled, not Missing"),
    }
}

#[test]
fn detail_returns_missing_when_enabled_but_provider_absent() {
    let _locale_guard = setup_locale();
    let mut settings = AppSettings::default();
    settings
        .provider
        .set_provider_enabled(ProviderKind::Gemini, true);

    let session = AppSession::new(settings, vec![]);
    let view = provider_detail_view_state(&session, &pid(ProviderKind::Gemini));

    assert!(
        matches!(view, ProviderDetailViewState::Missing { .. }),
        "expected Missing variant"
    );
}

// ── provider_body_view_state 分支测试 ────────────────────

#[test]
fn body_returns_error_empty_when_error_and_no_quotas() {
    let _locale_guard = setup_locale();
    let mut settings = AppSettings::default();
    settings
        .provider
        .set_provider_enabled(ProviderKind::Gemini, true);

    let mut provider = make_provider(ProviderKind::Gemini, ConnectionStatus::Error);
    provider.error_message = Some("API key invalid".to_string());
    // quotas 为空 → 走 Error + empty 分支

    let session = make_session_with_provider(settings, provider);
    let view = provider_detail_view_state(&session, &pid(ProviderKind::Gemini));

    match view {
        ProviderDetailViewState::Panel(panel) => match panel.body {
            ProviderBodyViewState::Empty(e) => {
                assert!(e.is_error);
                assert_eq!(e.message, "API key invalid");
            }
            other => panic!("expected Empty body, got {:?}", other),
        },
        _ => panic!("expected Panel variant"),
    }
}

#[test]
fn body_returns_refreshing_when_connection_is_refreshing() {
    let _locale_guard = setup_locale();
    let mut settings = AppSettings::default();
    settings
        .provider
        .set_provider_enabled(ProviderKind::Gemini, true);

    let provider = make_provider(ProviderKind::Gemini, ConnectionStatus::Refreshing);

    let session = make_session_with_provider(settings, provider);
    let view = provider_detail_view_state(&session, &pid(ProviderKind::Gemini));

    match view {
        ProviderDetailViewState::Panel(panel) => {
            assert!(
                matches!(panel.body, ProviderBodyViewState::Refreshing { .. }),
                "expected Refreshing body"
            );
        }
        _ => panic!("expected Panel variant"),
    }
}

#[test]
fn body_returns_quotas_when_visible_quotas_exist() {
    let _locale_guard = setup_locale();
    let mut settings = AppSettings::default();
    settings
        .provider
        .set_provider_enabled(ProviderKind::Gemini, true);

    let mut provider = make_provider(ProviderKind::Gemini, ConnectionStatus::Connected);
    provider.quotas = vec![QuotaInfo::new("requests", 50.0, 100.0)];

    let session = make_session_with_provider(settings, provider);
    let view = provider_detail_view_state(&session, &pid(ProviderKind::Gemini));

    match view {
        ProviderDetailViewState::Panel(panel) => match panel.body {
            ProviderBodyViewState::Quotas { quotas, generation } => {
                assert_eq!(quotas.len(), 1);
                assert_eq!(generation, session.nav.generation);
            }
            other => panic!("expected Quotas body, got {:?}", other),
        },
        _ => panic!("expected Panel variant"),
    }
}

#[test]
fn body_returns_empty_when_all_quotas_hidden() {
    let _locale_guard = setup_locale();
    let mut settings = AppSettings::default();
    settings
        .provider
        .set_provider_enabled(ProviderKind::Gemini, true);
    // 隐藏 general 类型的配额
    settings
        .provider
        .toggle_quota_visibility(ProviderKind::Gemini, "general".to_string());

    let mut provider = make_provider(ProviderKind::Gemini, ConnectionStatus::Connected);
    provider.quotas = vec![QuotaInfo::new("requests", 50.0, 100.0)]; // QuotaType::General

    let session = make_session_with_provider(settings, provider);
    let view = provider_detail_view_state(&session, &pid(ProviderKind::Gemini));

    match view {
        ProviderDetailViewState::Panel(panel) => match panel.body {
            ProviderBodyViewState::Empty(e) => {
                assert!(!e.is_error, "should not be error state");
            }
            other => panic!("expected Empty body, got {:?}", other),
        },
        _ => panic!("expected Panel variant"),
    }
}

#[test]
fn body_prefers_cached_quotas_over_error_empty() {
    let _locale_guard = setup_locale();
    let mut settings = AppSettings::default();
    settings
        .provider
        .set_provider_enabled(ProviderKind::Gemini, true);

    let mut provider = make_provider(ProviderKind::Gemini, ConnectionStatus::Error);
    provider.error_message = Some("timeout".to_string());
    // 有缓存配额 → 即使出错也应展示配额而非 Empty
    provider.quotas = vec![QuotaInfo::new("requests", 50.0, 100.0)];

    let session = make_session_with_provider(settings, provider);
    let view = provider_detail_view_state(&session, &pid(ProviderKind::Gemini));

    match view {
        ProviderDetailViewState::Panel(panel) => match panel.body {
            ProviderBodyViewState::Quotas { quotas, .. } => {
                assert_eq!(
                    quotas.len(),
                    1,
                    "cached quotas should be shown despite error"
                );
            }
            other => panic!("expected Quotas body (cached), got {:?}", other),
        },
        _ => panic!("expected Panel variant"),
    }
}

// ── QuotaDisplayMode 透传 ────────────────────────────

#[test]
fn panel_inherits_quota_display_mode_from_settings() {
    use crate::models::QuotaDisplayMode;

    let _locale_guard = setup_locale();
    let mut settings = AppSettings::default();
    settings
        .provider
        .set_provider_enabled(ProviderKind::Gemini, true);
    settings.display.quota_display_mode = QuotaDisplayMode::Used;

    let mut provider = make_provider(ProviderKind::Gemini, ConnectionStatus::Connected);
    provider.quotas = vec![QuotaInfo::new("test", 50.0, 100.0)];

    let session = make_session_with_provider(settings, provider);
    let view = provider_detail_view_state(&session, &pid(ProviderKind::Gemini));

    match view {
        ProviderDetailViewState::Panel(panel) => {
            assert_eq!(panel.quota_display_mode, QuotaDisplayMode::Used);
        }
        _ => panic!("expected Panel variant"),
    }
}

#[test]
fn panel_defaults_to_remaining_mode() {
    let _locale_guard = setup_locale();
    let mut settings = AppSettings::default();
    settings
        .provider
        .set_provider_enabled(ProviderKind::Gemini, true);

    let mut provider = make_provider(ProviderKind::Gemini, ConnectionStatus::Connected);
    provider.quotas = vec![QuotaInfo::new("test", 50.0, 100.0)];

    let session = make_session_with_provider(settings, provider);
    let view = provider_detail_view_state(&session, &pid(ProviderKind::Gemini));

    match view {
        ProviderDetailViewState::Panel(panel) => {
            assert_eq!(
                panel.quota_display_mode,
                crate::models::QuotaDisplayMode::Remaining
            );
        }
        _ => panic!("expected Panel variant"),
    }
}
