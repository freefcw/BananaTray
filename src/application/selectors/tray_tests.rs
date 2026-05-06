#![allow(deprecated)]
use super::*;
use crate::models::test_helpers::{
    make_test_provider as make_provider, setup_test_locale as setup_locale,
};
use crate::models::{
    AppSettings, ConnectionStatus, FailureReason, ProviderFailure, ProviderKind, QuotaInfo,
};

fn pid(kind: ProviderKind) -> ProviderId {
    ProviderId::BuiltIn(kind)
}

fn make_session_with_provider(mut settings: AppSettings, provider: ProviderStatus) -> AppSession {
    let id = provider.provider_id.clone();
    // 测试中关闭 overview，确保 active_tab 直接指向 Provider
    settings.display.show_overview = false;
    let session = AppSession::new(settings, vec![provider]);
    assert_eq!(session.nav.active_tab, NavTab::Provider(id));
    session
}

fn test_failure(message: &str) -> ProviderFailure {
    ProviderFailure {
        reason: FailureReason::FetchFailed,
        advice: None,
        raw_detail: Some(message.to_string()),
    }
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

    assert!(
        matches!(actions.refresh.target, Some(RefreshTarget::One(ref id)) if *id == pid(ProviderKind::Gemini)),
        "expected RefreshTarget::One(Gemini)"
    );
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
fn global_actions_hide_refresh_for_non_monitorable_provider() {
    let _locale_guard = setup_locale();
    let mut settings = AppSettings::default();
    settings
        .provider
        .set_provider_enabled(ProviderKind::Kilo, true);

    let provider = make_provider(ProviderKind::Kilo, ConnectionStatus::Disconnected);
    let session = make_session_with_provider(settings, provider);
    let actions = tray_global_actions_view_state(&session);

    assert!(!actions.show_refresh);
    assert!(actions.refresh.target.is_none());
}

#[test]
fn global_actions_refresh_id_none_on_settings_tab() {
    let _locale_guard = setup_locale();
    let settings = AppSettings::default();
    // 没有任何 provider → active_tab 回退到 Settings
    let mut session = AppSession::new(settings, vec![]);
    session.nav.active_tab = NavTab::Settings;
    let actions = tray_global_actions_view_state(&session);

    assert!(actions.refresh.target.is_none());
}

#[test]
fn global_actions_refresh_all_on_overview_tab() {
    let _locale_guard = setup_locale();
    let mut settings = AppSettings::default();
    settings.display.show_overview = true;
    settings
        .provider
        .set_provider_enabled(ProviderKind::Gemini, true);

    let provider = make_provider(ProviderKind::Gemini, ConnectionStatus::Connected);
    let session = AppSession::new(settings, vec![provider]);
    // show_overview = true → active_tab 默认为 Overview
    assert_eq!(session.nav.active_tab, NavTab::Overview);

    let actions = tray_global_actions_view_state(&session);
    assert!(matches!(actions.refresh.target, Some(RefreshTarget::All)));
    assert!(!actions.refresh.is_refreshing);
}

#[test]
fn global_actions_overview_is_refreshing_when_any_provider_refreshing() {
    let _locale_guard = setup_locale();
    let mut settings = AppSettings::default();
    settings.display.show_overview = true;
    settings
        .provider
        .set_provider_enabled(ProviderKind::Gemini, true);
    settings
        .provider
        .set_provider_enabled(ProviderKind::Claude, true);

    let gemini = make_provider(ProviderKind::Gemini, ConnectionStatus::Connected);
    let claude = make_provider(ProviderKind::Claude, ConnectionStatus::Refreshing);
    let session = AppSession::new(settings, vec![gemini, claude]);

    let actions = tray_global_actions_view_state(&session);
    assert!(matches!(actions.refresh.target, Some(RefreshTarget::All)));
    assert!(actions.refresh.is_refreshing);
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
fn detail_non_monitorable_provider_has_no_retry_action() {
    let _locale_guard = setup_locale();
    let mut settings = AppSettings::default();
    settings
        .provider
        .set_provider_enabled(ProviderKind::Kilo, true);

    let provider = make_provider(ProviderKind::Kilo, ConnectionStatus::Disconnected);
    let session = make_session_with_provider(settings, provider);
    let view = provider_detail_view_state(&session, &pid(ProviderKind::Kilo));

    match view {
        ProviderDetailViewState::Panel(panel) => match panel.body {
            ProviderBodyViewState::Empty(empty) => {
                assert_eq!(empty.title, "Monitoring unavailable");
                assert!(empty.action.is_none());
            }
            other => panic!(
                "expected Empty body, got {:?}",
                std::mem::discriminant(&other)
            ),
        },
        _ => panic!("expected Panel variant"),
    }
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
    provider.last_failure = Some(test_failure("API key invalid"));
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
        .toggle_quota_visibility(&pid(ProviderKind::Gemini), "requests".to_string());

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
    provider.last_failure = Some(test_failure("timeout"));
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

// ── overview_view_state ──────────────────────────────────

/// 辅助函数：构建 Overview 测试 session（开启 overview）
fn make_overview_session(providers: Vec<ProviderStatus>, enabled: &[ProviderKind]) -> AppSession {
    let mut settings = AppSettings::default();
    settings.display.show_overview = true;
    for k in enabled {
        settings.provider.set_provider_enabled(*k, true);
    }
    AppSession::new(settings, providers)
}

#[test]
fn overview_empty_when_no_enabled_providers() {
    let _locale_guard = setup_locale();
    let session = make_overview_session(vec![], &[]);
    let vm = overview_view_state(&session);
    assert!(vm.items.is_empty());
}

#[test]
fn overview_shows_quota_for_connected_provider() {
    let _locale_guard = setup_locale();
    let mut provider = make_provider(ProviderKind::Claude, ConnectionStatus::Connected);
    provider.quotas = vec![QuotaInfo::new("session", 30.0, 100.0)];

    let session = make_overview_session(vec![provider], &[ProviderKind::Claude]);
    let vm = overview_view_state(&session);

    assert_eq!(vm.items.len(), 1);
    let item = &vm.items[0];
    assert_eq!(item.id, pid(ProviderKind::Claude));
    match &item.status {
        OverviewItemStatus::Quota {
            status_level,
            quotas,
        } => {
            assert_eq!(*status_level, crate::models::StatusLevel::Green);
            assert_eq!(quotas.len(), 1);
            assert_eq!(quotas[0].display_text, "70%"); // 70% remaining (default mode)
            assert!(quotas[0].bar_ratio > 0.0 && quotas[0].bar_ratio <= 1.0);
        }
        other => panic!("expected Quota, got {:?}", other),
    }
}

#[test]
fn overview_shows_refreshing_status() {
    let _locale_guard = setup_locale();
    let provider = make_provider(ProviderKind::Gemini, ConnectionStatus::Refreshing);

    let session = make_overview_session(vec![provider], &[ProviderKind::Gemini]);
    let vm = overview_view_state(&session);

    assert_eq!(vm.items.len(), 1);
    assert!(matches!(vm.items[0].status, OverviewItemStatus::Refreshing));
}

#[test]
fn overview_shows_error_when_no_quotas() {
    let _locale_guard = setup_locale();
    let mut provider = make_provider(ProviderKind::Claude, ConnectionStatus::Error);
    provider.last_failure = Some(test_failure("auth expired"));
    // quotas 为空 → Error 分支

    let session = make_overview_session(vec![provider], &[ProviderKind::Claude]);
    let vm = overview_view_state(&session);

    assert_eq!(vm.items.len(), 1);
    match &vm.items[0].status {
        OverviewItemStatus::Error { message } => {
            assert_eq!(message, "auth expired");
        }
        other => panic!("expected Error, got {:?}", other),
    }
}

#[test]
fn overview_non_monitorable_provider_uses_informational_error_state() {
    let _locale_guard = setup_locale();
    let mut settings = AppSettings::default();
    settings.display.show_overview = true;
    settings
        .provider
        .set_provider_enabled(ProviderKind::Kilo, true);

    let provider = make_provider(ProviderKind::Kilo, ConnectionStatus::Disconnected);
    let session = AppSession::new(settings, vec![provider]);
    let overview = overview_view_state(&session);

    match &overview.items[0].status {
        OverviewItemStatus::Error { message } => {
            assert!(message.contains("does not support usage monitoring"));
        }
        _ => panic!("expected Error status for non-monitorable provider"),
    }
}

#[test]
fn overview_prefers_cached_quotas_over_error() {
    let _locale_guard = setup_locale();
    let mut provider = make_provider(ProviderKind::Claude, ConnectionStatus::Error);
    provider.last_failure = Some(test_failure("timeout"));
    // 有缓存配额 → 即使出错也应展示 Quota
    provider.quotas = vec![QuotaInfo::new("session", 50.0, 100.0)];

    let session = make_overview_session(vec![provider], &[ProviderKind::Claude]);
    let vm = overview_view_state(&session);

    assert_eq!(vm.items.len(), 1);
    assert!(
        matches!(vm.items[0].status, OverviewItemStatus::Quota { .. }),
        "cached quotas should trump error status"
    );
}

#[test]
fn overview_shows_disconnected_status() {
    let _locale_guard = setup_locale();
    let provider = make_provider(ProviderKind::Copilot, ConnectionStatus::Disconnected);

    let session = make_overview_session(vec![provider], &[ProviderKind::Copilot]);
    let vm = overview_view_state(&session);

    assert_eq!(vm.items.len(), 1);
    assert!(matches!(
        vm.items[0].status,
        OverviewItemStatus::Disconnected
    ));
}

#[test]
fn overview_picks_worst_quota_and_sorts_descending() {
    let _locale_guard = setup_locale();
    let mut provider = make_provider(ProviderKind::Claude, ConnectionStatus::Connected);
    provider.quotas = vec![
        QuotaInfo::new("session", 10.0, 100.0), // 90% remaining → Green
        QuotaInfo::new("weekly", 95.0, 100.0),  // 5% remaining → Red
    ];

    let session = make_overview_session(vec![provider], &[ProviderKind::Claude]);
    let vm = overview_view_state(&session);

    assert_eq!(vm.items.len(), 1);
    match &vm.items[0].status {
        OverviewItemStatus::Quota {
            status_level,
            quotas,
        } => {
            // overall worst 是 Red
            assert_eq!(*status_level, crate::models::StatusLevel::Red);
            // 收集了 2 个配额
            assert_eq!(quotas.len(), 2);
            // 最差的在前（Red）
            assert_eq!(quotas[0].status_level, crate::models::StatusLevel::Red);
            assert_eq!(quotas[0].label, "weekly");
            // 较好的在后（Green）
            assert_eq!(quotas[1].status_level, crate::models::StatusLevel::Green);
            assert_eq!(quotas[1].label, "session");
        }
        other => panic!("expected Quota, got {:?}", other),
    }
}

#[test]
fn overview_multi_quota_display_text_precomputed() {
    let _locale_guard = setup_locale();
    let mut provider = make_provider(ProviderKind::Claude, ConnectionStatus::Connected);
    provider.quotas = vec![
        QuotaInfo::new("fast", 40.0, 100.0), // 60% remaining
        QuotaInfo::new("slow", 80.0, 100.0), // 20% remaining
    ];

    let session = make_overview_session(vec![provider], &[ProviderKind::Claude]);
    let vm = overview_view_state(&session);

    match &vm.items[0].status {
        OverviewItemStatus::Quota { quotas, .. } => {
            // 每个配额都应有预计算的 display_text
            for q in quotas {
                assert!(
                    !q.display_text.is_empty(),
                    "display_text should be precomputed"
                );
                assert!(q.bar_ratio >= 0.0 && q.bar_ratio <= 1.0);
            }
        }
        other => panic!("expected Quota, got {:?}", other),
    }
}

#[test]
fn overview_excludes_disabled_providers() {
    let _locale_guard = setup_locale();
    let claude = make_provider(ProviderKind::Claude, ConnectionStatus::Connected);
    let gemini = make_provider(ProviderKind::Gemini, ConnectionStatus::Connected);

    // 只启用 Claude，不启用 Gemini
    let session = make_overview_session(vec![claude, gemini], &[ProviderKind::Claude]);
    let vm = overview_view_state(&session);

    assert_eq!(vm.items.len(), 1);
    assert_eq!(vm.items[0].id, pid(ProviderKind::Claude));
}

#[test]
fn overview_three_plus_quotas_collected_and_sorted() {
    let _locale_guard = setup_locale();
    let mut provider = make_provider(ProviderKind::Claude, ConnectionStatus::Connected);
    provider.quotas = vec![
        QuotaInfo::new("session", 10.0, 100.0), // 90% remaining → Green
        QuotaInfo::new("weekly", 95.0, 100.0),  // 5% remaining → Red
        QuotaInfo::new("monthly", 60.0, 100.0), // 40% remaining → Yellow
    ];

    let session = make_overview_session(vec![provider], &[ProviderKind::Claude]);
    let vm = overview_view_state(&session);

    assert_eq!(vm.items.len(), 1);
    match &vm.items[0].status {
        OverviewItemStatus::Quota {
            status_level,
            quotas,
        } => {
            // overall worst 应为 Red
            assert_eq!(*status_level, crate::models::StatusLevel::Red);
            // 收集了全部 3 个配额
            assert_eq!(quotas.len(), 3);
            // 按 status_level 降序排列：Red > Yellow > Green
            assert_eq!(quotas[0].status_level, crate::models::StatusLevel::Red);
            assert_eq!(quotas[0].label, "weekly");
            assert_eq!(quotas[1].status_level, crate::models::StatusLevel::Yellow);
            assert_eq!(quotas[1].label, "monthly");
            assert_eq!(quotas[2].status_level, crate::models::StatusLevel::Green);
            assert_eq!(quotas[2].label, "session");
            // 所有配额都有预计算的 display_text 和有效的 bar_ratio
            for q in quotas {
                assert!(!q.display_text.is_empty());
                assert!(q.bar_ratio >= 0.0 && q.bar_ratio <= 1.0);
            }
        }
        other => panic!("expected Quota, got {:?}", other),
    }
}

// ── header_view_state ──────────────────────────────────────

#[test]
fn header_view_state_synced() {
    let _locale_guard = setup_locale();
    let mut settings = AppSettings::default();
    settings
        .provider
        .set_provider_enabled(ProviderKind::Claude, true);

    let mut provider = make_provider(ProviderKind::Claude, ConnectionStatus::Connected);
    provider.last_refreshed_instant = Some(std::time::Instant::now());
    let session = make_session_with_provider(settings, provider);
    let header = header_view_state(&session);

    assert_eq!(header.status_kind, HeaderStatusKind::Synced);
    assert_eq!(header.status_text, "Synced");
}

#[test]
fn header_view_state_syncing() {
    let _locale_guard = setup_locale();
    let mut settings = AppSettings::default();
    settings
        .provider
        .set_provider_enabled(ProviderKind::Claude, true);

    let provider = make_provider(ProviderKind::Claude, ConnectionStatus::Refreshing);
    let session = make_session_with_provider(settings, provider);
    let header = header_view_state(&session);

    assert_eq!(header.status_kind, HeaderStatusKind::Syncing);
    assert_eq!(header.status_text, "Syncing…");
}

#[test]
fn header_view_state_offline() {
    let _locale_guard = setup_locale();
    let mut settings = AppSettings::default();
    settings
        .provider
        .set_provider_enabled(ProviderKind::Claude, true);

    let provider = make_provider(ProviderKind::Claude, ConnectionStatus::Disconnected);
    let session = make_session_with_provider(settings, provider);
    let header = header_view_state(&session);

    assert_eq!(header.status_kind, HeaderStatusKind::Offline);
    assert_eq!(header.status_text, "Offline");
}

#[test]
fn header_view_state_stale_minutes() {
    let _locale_guard = setup_locale();
    let mut settings = AppSettings::default();
    settings
        .provider
        .set_provider_enabled(ProviderKind::Claude, true);

    let mut provider = make_provider(ProviderKind::Claude, ConnectionStatus::Connected);
    provider.last_refreshed_instant =
        Some(std::time::Instant::now() - std::time::Duration::from_secs(300));
    let session = make_session_with_provider(settings, provider);
    let header = header_view_state(&session);

    assert_eq!(header.status_kind, HeaderStatusKind::Stale);
    assert_eq!(header.status_text, "5m ago");
}

#[test]
fn header_view_state_stale_hours() {
    let _locale_guard = setup_locale();
    let mut settings = AppSettings::default();
    settings
        .provider
        .set_provider_enabled(ProviderKind::Claude, true);

    let mut provider = make_provider(ProviderKind::Claude, ConnectionStatus::Connected);
    provider.last_refreshed_instant =
        Some(std::time::Instant::now() - std::time::Duration::from_secs(7200));
    let session = make_session_with_provider(settings, provider);
    let header = header_view_state(&session);

    assert_eq!(header.status_kind, HeaderStatusKind::Stale);
    assert_eq!(header.status_text, "2h ago");
}
