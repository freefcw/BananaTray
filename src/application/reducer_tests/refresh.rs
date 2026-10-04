use super::common::{
    has_effect, has_render, make_custom_provider_status, make_custom_token_provider, make_session,
    pid,
};
use crate::application::{
    reduce, AppAction, AppEffect, AppSession, CommonEffect, ContextEffect, NotificationEffect,
    QuotaNotificationEvent, RefreshEffect, SettingChange, SettingsEffect, TrayIconRequest,
};
use crate::models::test_helpers::make_test_provider;
use crate::models::{ConnectionStatus, NavTab, ProviderId, ProviderKind, QuotaInfo, RefreshData};
use crate::refresh::{RefreshEvent, RefreshOutcome, RefreshRequest, RefreshResult};

#[test]
fn refresh_success_in_dynamic_mode_produces_tray_icon_effect() {
    use crate::models::{QuotaInfo, StatusLevel, TrayIconStyle};

    let mut session = make_session();
    session.settings.display.tray_icon_style = TrayIconStyle::Dynamic;
    session
        .settings
        .provider
        .set_enabled(&pid(ProviderKind::Claude), true);
    // make_session 的 last_provider_id = Claude

    let effects = reduce(
        &mut session,
        AppAction::RefreshEventReceived(RefreshEvent::Finished(RefreshOutcome {
            reason: None,
            id: pid(ProviderKind::Claude),
            result: RefreshResult::Success {
                data: RefreshData {
                    quotas: vec![QuotaInfo::new("session", 95.0, 100.0)],
                    account_email: None,
                    account_tier: None,
                    source_label: None,
                },
            },
        })),
    );

    // 当前 Provider Claude 变 Red → 产出 ApplyTrayIcon
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Context(ContextEffect::ApplyTrayIcon(
            TrayIconRequest::DynamicStatus(StatusLevel::Red)
        ))
    )));
}

#[test]
fn refresh_success_in_static_mode_does_not_produce_tray_icon_effect() {
    use crate::models::{QuotaInfo, TrayIconStyle};

    let mut session = make_session();
    session.settings.display.tray_icon_style = TrayIconStyle::Yellow; // 静态模式
    session
        .settings
        .provider
        .set_enabled(&pid(ProviderKind::Claude), true);

    let effects = reduce(
        &mut session,
        AppAction::RefreshEventReceived(RefreshEvent::Finished(RefreshOutcome {
            reason: None,
            id: pid(ProviderKind::Claude),
            result: RefreshResult::Success {
                data: RefreshData {
                    quotas: vec![QuotaInfo::new("session", 95.0, 100.0)],
                    account_email: None,
                    account_tier: None,
                    source_label: None,
                },
            },
        })),
    );

    // 静态模式下不应产出 ApplyTrayIcon effect
    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Context(ContextEffect::ApplyTrayIcon(_))
    )));
}

#[test]
fn refresh_success_in_dynamic_mode_no_effect_when_status_unchanged() {
    use crate::models::{QuotaInfo, TrayIconStyle};

    let mut session = make_session();
    session.settings.display.tray_icon_style = TrayIconStyle::Dynamic;
    session
        .settings
        .provider
        .set_enabled(&pid(ProviderKind::Claude), true);

    // 第一次刷新：Green → Red，产出 effect
    reduce(
        &mut session,
        AppAction::RefreshEventReceived(RefreshEvent::Finished(RefreshOutcome {
            reason: None,
            id: pid(ProviderKind::Claude),
            result: RefreshResult::Success {
                data: RefreshData {
                    quotas: vec![QuotaInfo::new("session", 95.0, 100.0)],
                    account_email: None,
                    account_tier: None,
                    source_label: None,
                },
            },
        })),
    );

    // 第二次刷新：Red → Red（状态不变），不应产出 ApplyTrayIcon
    let effects = reduce(
        &mut session,
        AppAction::RefreshEventReceived(RefreshEvent::Finished(RefreshOutcome {
            reason: None,
            id: pid(ProviderKind::Claude),
            result: RefreshResult::Success {
                data: RefreshData {
                    quotas: vec![QuotaInfo::new("session", 96.0, 100.0)], // 仍是 Red
                    account_email: None,
                    account_tier: None,
                    source_label: None,
                },
            },
        })),
    );

    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Context(ContextEffect::ApplyTrayIcon(_))
    )));
}

#[test]
fn refresh_non_selected_enabled_provider_produces_tray_icon_effect() {
    use crate::models::{QuotaInfo, StatusLevel, TrayIconStyle};

    let mut session = make_session();
    session.settings.display.tray_icon_style = TrayIconStyle::Dynamic;
    // 当前选中 Claude（默认），但刷新的是已启用的 Gemini：
    // 聚合语义下未选中的 Provider 同样决定图标颜色
    session
        .settings
        .provider
        .set_enabled(&pid(ProviderKind::Gemini), true);

    let effects = reduce(
        &mut session,
        AppAction::RefreshEventReceived(RefreshEvent::Finished(RefreshOutcome {
            reason: None,
            id: pid(ProviderKind::Gemini),
            result: RefreshResult::Success {
                data: RefreshData {
                    quotas: vec![QuotaInfo::new("session", 95.0, 100.0)],
                    account_email: None,
                    account_tier: None,
                    source_label: None,
                },
            },
        })),
    );

    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Context(ContextEffect::ApplyTrayIcon(
            TrayIconRequest::DynamicStatus(StatusLevel::Red)
        ))
    )));
}

#[test]
fn refresh_disabled_provider_ignores_late_result() {
    use crate::models::{QuotaInfo, TrayIconStyle};

    let mut session = make_session();
    session.settings.display.tray_icon_style = TrayIconStyle::Dynamic;
    let provider_id = pid(ProviderKind::Gemini);
    let effects = reduce(
        &mut session,
        AppAction::RefreshEventReceived(RefreshEvent::Finished(RefreshOutcome {
            reason: None,
            id: provider_id.clone(),
            result: RefreshResult::Success {
                data: RefreshData {
                    quotas: vec![QuotaInfo::new("session", 95.0, 100.0)],
                    account_email: None,
                    account_tier: None,
                    source_label: None,
                },
            },
        })),
    );

    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Context(ContextEffect::ApplyTrayIcon(_))
    )));
    assert!(!session
        .provider_store
        .find_by_id(&provider_id)
        .unwrap()
        .quotas
        .iter()
        .any(|quota| quota.used == 95.0 && quota.limit == 100.0));
}

#[test]
fn refresh_deferred_while_popup_visible() {
    use crate::models::{QuotaInfo, TrayIconStyle};

    let mut session = make_session();
    session.settings.display.tray_icon_style = TrayIconStyle::Dynamic;
    session.popup_visible = true; // 弹窗打开

    let effects = reduce(
        &mut session,
        AppAction::RefreshEventReceived(RefreshEvent::Finished(RefreshOutcome {
            reason: None,
            id: pid(ProviderKind::Claude),
            result: RefreshResult::Success {
                data: RefreshData {
                    quotas: vec![QuotaInfo::new("session", 95.0, 100.0)],
                    account_email: None,
                    account_tier: None,
                    source_label: None,
                },
            },
        })),
    );

    // 弹窗可见时不产出 ApplyTrayIcon
    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Context(ContextEffect::ApplyTrayIcon(_))
    )));
}

// ── RefreshAll ──────────────────────────────────────

#[test]
fn refresh_all_marks_enabled_providers_refreshing() {
    let mut session = make_session();
    session
        .settings
        .provider
        .set_enabled(&pid(ProviderKind::Claude), true);
    session
        .settings
        .provider
        .set_enabled(&pid(ProviderKind::Gemini), true);

    let effects = reduce(&mut session, AppAction::RefreshAll);

    // 所有已启用的 provider 应被标记为 Refreshing
    let claude = session
        .provider_store
        .find_by_id(&pid(ProviderKind::Claude))
        .unwrap();
    assert_eq!(claude.connection, ConnectionStatus::Refreshing);
    let gemini = session
        .provider_store
        .find_by_id(&pid(ProviderKind::Gemini))
        .unwrap();
    assert_eq!(gemini.connection, ConnectionStatus::Refreshing);

    // 未启用的 provider 不受影响
    let copilot = session
        .provider_store
        .find_by_id(&pid(ProviderKind::Copilot))
        .unwrap();
    assert_ne!(copilot.connection, ConnectionStatus::Refreshing);

    // 应产出 RefreshAll 请求
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Refresh(RefreshEffect::SendRequest(
            RefreshRequest::RefreshAll { .. }
        )))
    )));
    assert!(has_render(&effects));
}

#[test]
fn refresh_all_with_no_enabled_providers_is_safe() {
    let mut session = make_session();
    // 默认没有启用任何 provider

    let effects = reduce(&mut session, AppAction::RefreshAll);

    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Refresh(RefreshEffect::SendRequest(
            RefreshRequest::RefreshAll { .. }
        )))
    )));
    assert!(!has_render(&effects));
}

#[test]
fn refresh_all_skips_non_monitorable_providers() {
    let mut session = make_session();
    let kilo_id = pid(ProviderKind::Kilo);
    session.settings.provider.set_enabled(&kilo_id, true);

    let effects = reduce(&mut session, AppAction::RefreshAll);

    let kilo = session.provider_store.find_by_id(&kilo_id).unwrap();
    assert_ne!(kilo.connection, ConnectionStatus::Refreshing);
    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Refresh(RefreshEffect::SendRequest(
            RefreshRequest::RefreshAll { .. }
        )))
    )));
    assert!(!has_render(&effects));
}

// ── ProvidersReloaded (热重载) ───────────────────────────

#[test]
fn providers_reloaded_sends_update_config() {
    let mut session = make_session();
    let statuses = session.provider_store.providers.to_vec();

    let effects = reduce(
        &mut session,
        AppAction::RefreshEventReceived(RefreshEvent::ProvidersReloaded { statuses }),
    );

    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Refresh(RefreshEffect::SendRequest(
            RefreshRequest::UpdateConfig { .. }
        )))
    )));
    assert!(has_render(&effects));
}

#[test]
fn providers_reloaded_refreshes_enabled_new_custom() {
    let mut session = make_session();
    let custom_id = ProviderId::Custom("new:api".to_string());
    session.settings.provider.set_enabled(&custom_id, true);

    let mut statuses = session.provider_store.providers.to_vec();
    statuses.push(make_custom_provider_status("new:api"));

    let effects = reduce(
        &mut session,
        AppAction::RefreshEventReceived(RefreshEvent::ProvidersReloaded { statuses }),
    );

    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Refresh(RefreshEffect::SendRequest(RefreshRequest::RefreshOne {
            ref id,
            ..
        }))) if *id == ProviderId::Custom("new:api".to_string())
    )));
}

#[test]
fn providers_reloaded_does_not_refresh_disabled_custom() {
    let mut session = make_session();

    // 明确禁用该 Provider（模拟用户手动关闭的场景）
    let custom_id = ProviderId::Custom("disabled:api".to_string());
    session.settings.provider.set_enabled(&custom_id, false);

    let mut statuses = session.provider_store.providers.to_vec();
    statuses.push(make_custom_provider_status("disabled:api"));

    let effects = reduce(
        &mut session,
        AppAction::RefreshEventReceived(RefreshEvent::ProvidersReloaded { statuses }),
    );

    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Refresh(RefreshEffect::SendRequest(RefreshRequest::RefreshOne {
            ref id,
            ..
        }))) if *id == ProviderId::Custom("disabled:api".to_string())
    )));
}

#[test]
fn providers_reloaded_clears_debug_selection_for_deleted_custom() {
    let mut session = make_session();
    let custom_id = ProviderId::Custom("old:api".to_string());
    session
        .provider_store
        .providers
        .push(make_custom_provider_status("old:api"));
    session.debug_ui.selected_provider = Some(custom_id);

    let statuses: Vec<_> = ProviderKind::all()
        .iter()
        .map(|k| make_test_provider(*k, ConnectionStatus::Disconnected))
        .collect();

    reduce(
        &mut session,
        AppAction::RefreshEventReceived(RefreshEvent::ProvidersReloaded { statuses }),
    );

    assert!(session.debug_ui.selected_provider.is_none());
}

#[test]
fn providers_reloaded_repoints_active_tab_when_deleted() {
    let mut session = make_session();
    let custom_id = ProviderId::Custom("gone:api".to_string());
    session
        .provider_store
        .providers
        .push(make_custom_provider_status("gone:api"));
    session.settings.provider.set_enabled(&custom_id, true);
    session.nav.switch_to(NavTab::Provider(custom_id.clone()));

    let statuses: Vec<_> = ProviderKind::all()
        .iter()
        .map(|k| make_test_provider(*k, ConnectionStatus::Disconnected))
        .collect();

    reduce(
        &mut session,
        AppAction::RefreshEventReceived(RefreshEvent::ProvidersReloaded { statuses }),
    );

    match &session.nav.active_tab {
        NavTab::Provider(id) => assert_ne!(*id, custom_id),
        NavTab::Settings | NavTab::Overview => {}
    }
}

#[test]
fn providers_reloaded_persists_settings_when_stale_ids_pruned() {
    let mut session = make_session();
    let custom_id = ProviderId::Custom("stale:api".to_string());
    session.settings.provider.set_enabled(&custom_id, true);
    session
        .provider_store
        .providers
        .push(make_custom_provider_status("stale:api"));

    let statuses: Vec<_> = ProviderKind::all()
        .iter()
        .map(|k| make_test_provider(*k, ConnectionStatus::Disconnected))
        .collect();

    let effects = reduce(
        &mut session,
        AppAction::RefreshEventReceived(RefreshEvent::ProvidersReloaded { statuses }),
    );

    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::PersistSettings))
    )));
}

// ── Skipped* 收敛 ─────────────────────────────────────

fn dispatch_skipped(session: &mut AppSession, result: RefreshResult) {
    reduce(
        session,
        AppAction::RefreshEventReceived(RefreshEvent::Finished(RefreshOutcome {
            reason: None,
            id: pid(ProviderKind::Claude),
            result,
        })),
    );
}

#[test]
fn skipped_cooldown_converges_refreshing_without_data_to_disconnected() {
    let mut session = make_session();
    session
        .settings
        .provider
        .set_enabled(&pid(ProviderKind::Claude), true);
    session
        .provider_store
        .mark_refreshing_by_id(&pid(ProviderKind::Claude));

    let effects = reduce(
        &mut session,
        AppAction::RefreshEventReceived(RefreshEvent::Finished(RefreshOutcome {
            reason: None,
            id: pid(ProviderKind::Claude),
            result: RefreshResult::SkippedCooldown,
        })),
    );

    let claude = session
        .provider_store
        .find_by_id(&pid(ProviderKind::Claude))
        .unwrap();
    assert_eq!(claude.connection, ConnectionStatus::Disconnected);
    assert!(has_render(&effects));
}

#[test]
fn skipped_result_converges_refreshing_with_stale_data_to_connected() {
    use crate::models::QuotaInfo;

    let mut session = make_session();
    session
        .settings
        .provider
        .set_enabled(&pid(ProviderKind::Claude), true);
    {
        let claude = session
            .provider_store
            .find_by_id_mut(&pid(ProviderKind::Claude))
            .unwrap();
        claude.quotas.push(QuotaInfo::new("session", 50.0, 100.0));
    }
    session
        .provider_store
        .mark_refreshing_by_id(&pid(ProviderKind::Claude));

    dispatch_skipped(&mut session, RefreshResult::SkippedCooldown);

    let claude = session
        .provider_store
        .find_by_id(&pid(ProviderKind::Claude))
        .unwrap();
    // 有旧数据 → 收敛回 Connected（展示陈旧数据），而不是停留在 Refreshing
    assert_eq!(claude.connection, ConnectionStatus::Connected);
}

#[test]
fn skipped_in_flight_and_disabled_also_converge() {
    for result in [
        RefreshResult::SkippedInFlight,
        RefreshResult::SkippedDisabled,
    ] {
        let mut s = make_session();
        s.settings
            .provider
            .set_enabled(&pid(ProviderKind::Claude), true);
        s.provider_store
            .mark_refreshing_by_id(&pid(ProviderKind::Claude));

        let effects = reduce(
            &mut s,
            AppAction::RefreshEventReceived(RefreshEvent::Finished(RefreshOutcome {
                reason: None,
                id: pid(ProviderKind::Claude),
                result,
            })),
        );

        let claude = s
            .provider_store
            .find_by_id(&pid(ProviderKind::Claude))
            .unwrap();
        assert_eq!(claude.connection, ConnectionStatus::Disconnected);
        assert!(has_render(&effects));
    }
}

#[test]
fn skipped_does_not_touch_non_refreshing_provider() {
    let mut session = make_session();
    session
        .settings
        .provider
        .set_enabled(&pid(ProviderKind::Claude), true);
    session
        .provider_store
        .find_by_id_mut(&pid(ProviderKind::Claude))
        .unwrap()
        .mark_refresh_succeeded(crate::models::RefreshData {
            quotas: vec![],
            account_email: None,
            account_tier: None,
            source_label: None,
        });

    let effects = reduce(
        &mut session,
        AppAction::RefreshEventReceived(RefreshEvent::Finished(RefreshOutcome {
            reason: None,
            id: pid(ProviderKind::Claude),
            result: RefreshResult::SkippedCooldown,
        })),
    );

    let claude = session
        .provider_store
        .find_by_id(&pid(ProviderKind::Claude))
        .unwrap();
    // 非 Refreshing 状态不受影响
    assert_eq!(claude.connection, ConnectionStatus::Connected);
    assert!(!has_render(&effects));
}

fn refresh_success(
    session: &mut AppSession,
    id: &ProviderId,
    remaining_pct: f64,
) -> Vec<AppEffect> {
    reduce(
        session,
        AppAction::RefreshEventReceived(RefreshEvent::Finished(RefreshOutcome {
            reason: None,
            id: id.clone(),
            result: RefreshResult::Success {
                data: RefreshData {
                    quotas: vec![QuotaInfo::new("session", 100.0 - remaining_pct, 100.0)],
                    account_email: None,
                    account_tier: None,
                    source_label: None,
                },
            },
        })),
    )
}

fn quota_alerts(effects: &[AppEffect]) -> Vec<&QuotaNotificationEvent> {
    effects
        .iter()
        .filter_map(|e| match e {
            AppEffect::Common(CommonEffect::Notification(NotificationEffect::Quota {
                event,
                ..
            })) => Some(event),
            _ => None,
        })
        .collect()
}

fn assert_usage_progress(
    alerts: &[&QuotaNotificationEvent],
    expected_name: &str,
    expected_remaining: f64,
) {
    assert_eq!(
        alerts.len(),
        1,
        "expected exactly one quota alert: {alerts:?}"
    );
    match alerts[0] {
        QuotaNotificationEvent::UsageProgress {
            provider_name,
            remaining_pct,
        } => {
            assert_eq!(provider_name, expected_name);
            assert!(
                (*remaining_pct - expected_remaining).abs() < 1e-9,
                "expected remaining {expected_remaining}, got {remaining_pct}"
            );
        }
        other => panic!("expected UsageProgress, got {other:?}"),
    }
}

fn enable(session: &mut AppSession, kind: ProviderKind) {
    session.settings.provider.set_enabled(&pid(kind), true);
}

#[test]
fn usage_step_global_default_zero_emits_no_progress() {
    let mut session = make_session();
    enable(&mut session, ProviderKind::Claude);

    refresh_success(&mut session, &pid(ProviderKind::Claude), 83.0);
    let effects = refresh_success(&mut session, &pid(ProviderKind::Claude), 60.0);
    assert!(quota_alerts(&effects).is_empty());
}

#[test]
fn usage_step_global_value_inherited_without_override() {
    let mut session = make_session();
    session.settings.notification.quota_usage_step_pct = 5;
    enable(&mut session, ProviderKind::Claude);
    let claude = pid(ProviderKind::Claude);

    refresh_success(&mut session, &claude, 83.0);
    assert!(quota_alerts(&refresh_success(&mut session, &claude, 79.0)).is_empty());
    assert_usage_progress(
        &quota_alerts(&refresh_success(&mut session, &claude, 78.0)),
        "Claude",
        78.0,
    );
}

#[test]
fn usage_step_provider_override_takes_precedence() {
    let mut session = make_session();
    session.settings.notification.quota_usage_step_pct = 5;
    let claude = pid(ProviderKind::Claude);
    session
        .settings
        .provider
        .set_quota_usage_step(&claude, Some(10));
    enable(&mut session, ProviderKind::Claude);

    refresh_success(&mut session, &claude, 90.0);
    assert!(quota_alerts(&refresh_success(&mut session, &claude, 85.0)).is_empty());
    assert_usage_progress(
        &quota_alerts(&refresh_success(&mut session, &claude, 80.0)),
        "Claude",
        80.0,
    );
}

#[test]
fn usage_step_provider_override_zero_disables() {
    let mut session = make_session();
    session.settings.notification.quota_usage_step_pct = 5;
    let claude = pid(ProviderKind::Claude);
    session
        .settings
        .provider
        .set_quota_usage_step(&claude, Some(0));
    enable(&mut session, ProviderKind::Claude);

    refresh_success(&mut session, &claude, 83.0);
    let effects = refresh_success(&mut session, &claude, 60.0);
    assert!(quota_alerts(&effects).is_empty());
}

#[test]
fn usage_step_global_zero_plus_provider_override_alerts() {
    let mut session = make_session();
    let claude = pid(ProviderKind::Claude);
    session
        .settings
        .provider
        .set_quota_usage_step(&claude, Some(5));
    enable(&mut session, ProviderKind::Claude);

    refresh_success(&mut session, &claude, 83.0);
    assert_usage_progress(
        &quota_alerts(&refresh_success(&mut session, &claude, 78.0)),
        "Claude",
        78.0,
    );
}

#[test]
fn usage_step_master_switch_off_suppresses_all_quota_effects() {
    let mut session = make_session();
    session.settings.notification.quota_usage_step_pct = 5;
    enable(&mut session, ProviderKind::Claude);
    let claude = pid(ProviderKind::Claude);

    refresh_success(&mut session, &claude, 83.0);
    reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::ToggleSessionQuotaNotifications),
    );

    let effects = refresh_success(&mut session, &claude, 8.0);
    assert!(quota_alerts(&effects).is_empty());
}

#[test]
fn usage_step_master_toggle_resets_baseline_without_refresh() {
    let mut session = make_session();
    session.settings.notification.quota_usage_step_pct = 5;
    enable(&mut session, ProviderKind::Claude);
    let claude = pid(ProviderKind::Claude);

    refresh_success(&mut session, &claude, 83.0);
    reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::ToggleSessionQuotaNotifications),
    );
    reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::ToggleSessionQuotaNotifications),
    );

    assert!(quota_alerts(&refresh_success(&mut session, &claude, 78.0)).is_empty());
    assert_usage_progress(
        &quota_alerts(&refresh_success(&mut session, &claude, 73.0)),
        "Claude",
        73.0,
    );
}

#[test]
fn usage_step_disable_reenable_resets_baseline() {
    let mut session = make_session();
    session.settings.notification.quota_usage_step_pct = 5;
    let claude = pid(ProviderKind::Claude);
    enable(&mut session, ProviderKind::Claude);

    refresh_success(&mut session, &claude, 83.0);
    reduce(&mut session, AppAction::ToggleProvider(claude.clone()));
    reduce(&mut session, AppAction::ToggleProvider(claude.clone()));

    assert!(quota_alerts(&refresh_success(&mut session, &claude, 78.0)).is_empty());
    assert_usage_progress(
        &quota_alerts(&refresh_success(&mut session, &claude, 73.0)),
        "Claude",
        73.0,
    );
}

#[test]
fn usage_step_remove_readd_sidebar_resets_baseline() {
    let mut session = make_session();
    session.settings.notification.quota_usage_step_pct = 5;
    let claude = pid(ProviderKind::Claude);
    enable(&mut session, ProviderKind::Claude);

    refresh_success(&mut session, &claude, 83.0);
    reduce(
        &mut session,
        AppAction::RemoveProviderFromSidebar(claude.clone()),
    );
    reduce(
        &mut session,
        AppAction::AddProviderToSidebar(claude.clone()),
    );
    reduce(&mut session, AppAction::ToggleProvider(claude.clone()));

    assert!(quota_alerts(&refresh_success(&mut session, &claude, 78.0)).is_empty());
    assert_usage_progress(
        &quota_alerts(&refresh_success(&mut session, &claude, 73.0)),
        "Claude",
        73.0,
    );
}

#[test]
fn usage_step_global_change_does_not_reset_overridden_provider() {
    let mut session = make_session();
    session.settings.notification.quota_usage_step_pct = 5;
    let claude = pid(ProviderKind::Claude);
    session
        .settings
        .provider
        .set_quota_usage_step(&claude, Some(10));
    enable(&mut session, ProviderKind::Claude);

    refresh_success(&mut session, &claude, 90.0);
    assert!(quota_alerts(&refresh_success(&mut session, &claude, 85.0)).is_empty());

    reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::SetQuotaUsageStep(8)),
    );
    assert_usage_progress(
        &quota_alerts(&refresh_success(&mut session, &claude, 80.0)),
        "Claude",
        80.0,
    );
}

#[test]
fn usage_step_settings_persist_render_without_config_sync() {
    let mut session = make_session();
    let claude = pid(ProviderKind::Claude);

    let effects = reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::SetQuotaUsageStep(10)),
    );
    assert_eq!(session.settings.notification.quota_usage_step_pct, 10);
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::PersistSettings))
    )));
    assert!(has_render(&effects));
    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Refresh(RefreshEffect::SendRequest(
            RefreshRequest::UpdateConfig { .. }
        )))
    )));

    let effects = reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::SetProviderQuotaUsageStep {
            provider_id: claude.clone(),
            step_pct: Some(5),
        }),
    );
    assert_eq!(session.settings.provider.quota_usage_step(&claude), Some(5));
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::PersistSettings))
    )));
    assert!(has_render(&effects));
    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Refresh(RefreshEffect::SendRequest(
            RefreshRequest::UpdateConfig { .. }
        )))
    )));
}

#[test]
fn usage_step_notification_sound_flag_passes_through() {
    let mut session = make_session();
    session.settings.notification.quota_usage_step_pct = 5;
    session.settings.notification.notification_sound = false;
    enable(&mut session, ProviderKind::Claude);
    let claude = pid(ProviderKind::Claude);

    refresh_success(&mut session, &claude, 83.0);
    let effects = refresh_success(&mut session, &claude, 78.0);

    let with_sound = effects.iter().find_map(|e| match e {
        AppEffect::Common(CommonEffect::Notification(NotificationEffect::Quota {
            event: QuotaNotificationEvent::UsageProgress { .. },
            with_sound,
        })) => Some(*with_sound),
        _ => None,
    });
    assert_eq!(with_sound, Some(false));
}

#[test]
fn usage_step_failed_skipped_disabled_results_do_not_advance_baseline() {
    use crate::models::{ErrorKind, FailureReason, ProviderFailure};

    let mut session = make_session();
    session.settings.notification.quota_usage_step_pct = 5;
    let claude = pid(ProviderKind::Claude);
    enable(&mut session, ProviderKind::Claude);

    refresh_success(&mut session, &claude, 83.0);
    reduce(
        &mut session,
        AppAction::RefreshEventReceived(RefreshEvent::Finished(RefreshOutcome {
            reason: None,
            id: claude.clone(),
            result: RefreshResult::Failed {
                failure: ProviderFailure {
                    reason: FailureReason::FetchFailed,
                    advice: None,
                    raw_detail: Some("boom".to_string()),
                },
                error_kind: ErrorKind::NetworkError,
            },
        })),
    );
    reduce(
        &mut session,
        AppAction::RefreshEventReceived(RefreshEvent::Finished(RefreshOutcome {
            reason: None,
            id: claude.clone(),
            result: RefreshResult::SkippedCooldown,
        })),
    );
    session.settings.provider.set_enabled(&claude, false);
    refresh_success(&mut session, &claude, 50.0);
    session.settings.provider.set_enabled(&claude, true);

    assert_usage_progress(
        &quota_alerts(&refresh_success(&mut session, &claude, 78.0)),
        "Claude",
        78.0,
    );
}

#[test]
fn usage_step_provider_override_change_rebuilds_baseline() {
    let mut session = make_session();
    session.settings.notification.quota_usage_step_pct = 5;
    let claude = pid(ProviderKind::Claude);
    enable(&mut session, ProviderKind::Claude);

    refresh_success(&mut session, &claude, 83.0);
    assert!(quota_alerts(&refresh_success(&mut session, &claude, 81.0)).is_empty());

    reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::SetProviderQuotaUsageStep {
            provider_id: claude.clone(),
            step_pct: Some(10),
        }),
    );

    assert!(quota_alerts(&refresh_success(&mut session, &claude, 71.0)).is_empty());
    assert_usage_progress(
        &quota_alerts(&refresh_success(&mut session, &claude, 61.0)),
        "Claude",
        61.0,
    );
}

#[test]
fn usage_step_equivalent_override_inheritance_keeps_baseline() {
    let mut session = make_session();
    session.settings.notification.quota_usage_step_pct = 5;
    let claude = pid(ProviderKind::Claude);
    session
        .settings
        .provider
        .set_quota_usage_step(&claude, Some(5));
    enable(&mut session, ProviderKind::Claude);

    refresh_success(&mut session, &claude, 83.0);
    assert!(quota_alerts(&refresh_success(&mut session, &claude, 81.0)).is_empty());

    reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::SetProviderQuotaUsageStep {
            provider_id: claude.clone(),
            step_pct: None,
        }),
    );

    assert_usage_progress(
        &quota_alerts(&refresh_success(&mut session, &claude, 78.0)),
        "Claude",
        78.0,
    );
}

#[test]
fn usage_step_token_change_rebuilds_baseline() {
    for (provider_id, credential_key, custom_status) in [
        (pid(ProviderKind::Copilot), "github_token", None),
        (
            ProviderId::Custom("custom-token:api".to_string()),
            "custom_token",
            Some(make_custom_token_provider(
                "custom-token:api",
                "custom_token",
            )),
        ),
    ] {
        let mut session = make_session();
        if let Some(status) = custom_status {
            session.provider_store.providers.push(status);
        }
        let provider_name = session
            .provider_store
            .find_by_id(&provider_id)
            .unwrap()
            .display_name()
            .to_string();
        session.settings.notification.quota_usage_step_pct = 5;
        session.settings.provider.set_enabled(&provider_id, true);
        session
            .settings
            .provider
            .credentials
            .set_credential(credential_key, "dummy-old".to_string());

        refresh_success(&mut session, &provider_id, 83.0);
        assert!(quota_alerts(&refresh_success(&mut session, &provider_id, 81.0)).is_empty());

        reduce(
            &mut session,
            AppAction::SaveProviderToken {
                provider_id: provider_id.clone(),
                token: "dummy-new".to_string(),
            },
        );

        assert!(
            quota_alerts(&refresh_success(&mut session, &provider_id, 78.0)).is_empty(),
            "{provider_id}: credential change must rebuild baseline without replaying consumption"
        );
        assert_usage_progress(
            &quota_alerts(&refresh_success(&mut session, &provider_id, 73.0)),
            &provider_name,
            73.0,
        );
    }
}

#[test]
fn usage_step_unchanged_token_preserves_baseline() {
    for (provider_id, credential_key, custom_status) in [
        (pid(ProviderKind::Copilot), "github_token", None),
        (
            ProviderId::Custom("custom-token:api".to_string()),
            "custom_token",
            Some(make_custom_token_provider(
                "custom-token:api",
                "custom_token",
            )),
        ),
    ] {
        let mut session = make_session();
        if let Some(status) = custom_status {
            session.provider_store.providers.push(status);
        }
        let provider_name = session
            .provider_store
            .find_by_id(&provider_id)
            .unwrap()
            .display_name()
            .to_string();
        session.settings.notification.quota_usage_step_pct = 5;
        session.settings.provider.set_enabled(&provider_id, true);
        session
            .settings
            .provider
            .credentials
            .set_credential(credential_key, "dummy-old".to_string());

        refresh_success(&mut session, &provider_id, 83.0);
        assert!(quota_alerts(&refresh_success(&mut session, &provider_id, 81.0)).is_empty());

        reduce(
            &mut session,
            AppAction::SaveProviderToken {
                provider_id: provider_id.clone(),
                token: "  dummy-old \n".to_string(),
            },
        );

        assert_usage_progress(
            &quota_alerts(&refresh_success(&mut session, &provider_id, 78.0)),
            &provider_name,
            78.0,
        );
    }
}

#[test]
fn usage_step_empty_token_preserves_baseline() {
    let copilot = pid(ProviderKind::Copilot);
    let provider_name;
    let mut session = make_session();
    {
        provider_name = session
            .provider_store
            .find_by_id(&copilot)
            .unwrap()
            .display_name()
            .to_string();
    }
    session.settings.notification.quota_usage_step_pct = 5;
    session.settings.provider.set_enabled(&copilot, true);
    session
        .settings
        .provider
        .credentials
        .set_credential("github_token", "dummy-old".to_string());

    refresh_success(&mut session, &copilot, 83.0);
    assert!(quota_alerts(&refresh_success(&mut session, &copilot, 81.0)).is_empty());

    let save_effects = reduce(
        &mut session,
        AppAction::SaveProviderToken {
            provider_id: copilot.clone(),
            token: "   \n".to_string(),
        },
    );

    assert!(!has_effect(&save_effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::PersistSettings))
    )));
    assert_eq!(
        session
            .settings
            .provider
            .credentials
            .get_credential("github_token"),
        Some("dummy-old")
    );
    assert_usage_progress(
        &quota_alerts(&refresh_success(&mut session, &copilot, 78.0)),
        &provider_name,
        78.0,
    );
}

#[test]
fn quota_threshold_save_only_persists_renders_and_publishes() {
    let mut session = make_session();

    let effects = reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::SetGlobalQuotaThresholds {
            unit: crate::models::QuotaThresholdUnit::Percentage,
            thresholds: crate::models::QuotaThresholds {
                warning: 60.0,
                critical: 30.0,
                notify: 15.0,
            },
        }),
    );

    assert_eq!(session.settings.quota.percentage.notify, 15.0);
    assert!(has_render(&effects));
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::PersistSettings))
    )));
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Context(ContextEffect::PublishQuotaSnapshot)
    )));
    assert!(quota_alerts(&effects).is_empty());
    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Refresh(RefreshEffect::SendRequest(_)))
    )));
}

#[test]
fn quota_threshold_invalid_group_is_rejected_without_side_effects() {
    let mut session = make_session();
    let before = session.settings.quota;

    let effects = reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::SetGlobalQuotaThresholds {
            unit: crate::models::QuotaThresholdUnit::Percentage,
            thresholds: crate::models::QuotaThresholds {
                warning: 60.0,
                critical: 20.0,
                notify: 30.0,
            },
        }),
    );

    assert!(effects.is_empty());
    assert_eq!(session.settings.quota, before);
}

#[test]
fn quota_threshold_notify_raise_rebaselines_without_alert() {
    let mut session = make_session();
    let claude = pid(ProviderKind::Claude);
    enable(&mut session, ProviderKind::Claude);

    refresh_success(&mut session, &claude, 30.0);

    let effects = reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::SetGlobalQuotaThresholds {
            unit: crate::models::QuotaThresholdUnit::Percentage,
            thresholds: crate::models::QuotaThresholds {
                warning: 80.0,
                critical: 40.0,
                notify: 30.0,
            },
        }),
    );
    assert!(quota_alerts(&effects).is_empty());

    assert!(quota_alerts(&refresh_success(&mut session, &claude, 29.0)).is_empty());
    assert!(matches!(
        quota_alerts(&refresh_success(&mut session, &claude, 0.0)).first(),
        Some(QuotaNotificationEvent::Exhausted { .. })
    ));
}

#[test]
fn quota_threshold_provider_override_isolated_from_global_change() {
    let mut session = make_session();
    let claude = pid(ProviderKind::Claude);
    let codex = pid(ProviderKind::Codex);
    enable(&mut session, ProviderKind::Claude);
    enable(&mut session, ProviderKind::Codex);

    session.settings.provider.set_quota_threshold_override(
        &claude,
        crate::models::QuotaThresholdUnit::Percentage,
        Some(crate::models::QuotaThresholds {
            warning: 80.0,
            critical: 40.0,
            notify: 30.0,
        }),
    );
    refresh_success(&mut session, &claude, 35.0);
    refresh_success(&mut session, &codex, 35.0);

    reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::SetGlobalQuotaThresholds {
            unit: crate::models::QuotaThresholdUnit::Percentage,
            thresholds: crate::models::QuotaThresholds {
                warning: 90.0,
                critical: 50.0,
                notify: 40.0,
            },
        }),
    );

    assert!(matches!(
        quota_alerts(&refresh_success(&mut session, &claude, 29.0)).first(),
        Some(QuotaNotificationEvent::LowQuota { .. })
    ));
    assert!(quota_alerts(&refresh_success(&mut session, &codex, 39.0)).is_empty());
}

#[test]
fn quota_threshold_identity_switch_keeps_usage_step_baseline() {
    let mut session = make_session();
    session.settings.notification.quota_usage_step_pct = 5;
    let claude = pid(ProviderKind::Claude);
    enable(&mut session, ProviderKind::Claude);

    refresh_success(&mut session, &claude, 83.0);

    let defaults = crate::models::QuotaThresholds::DEFAULT_PERCENTAGE;
    reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::SetProviderQuotaThresholds {
            provider_id: claude.clone(),
            unit: crate::models::QuotaThresholdUnit::Percentage,
            thresholds: Some(defaults),
        }),
    );
    reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::SetProviderQuotaThresholds {
            provider_id: claude.clone(),
            unit: crate::models::QuotaThresholdUnit::Percentage,
            thresholds: None,
        }),
    );

    assert_usage_progress(
        &quota_alerts(&refresh_success(&mut session, &claude, 78.0)),
        "Claude",
        78.0,
    );
}

#[test]
fn quota_threshold_provider_save_does_not_rebaseline_other_provider() {
    let mut session = make_session();
    session.settings.notification.quota_usage_step_pct = 5;
    let claude = pid(ProviderKind::Claude);
    let codex = pid(ProviderKind::Codex);
    enable(&mut session, ProviderKind::Claude);
    enable(&mut session, ProviderKind::Codex);

    refresh_success(&mut session, &codex, 83.0);
    refresh_success(&mut session, &codex, 81.0);

    reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::SetProviderQuotaThresholds {
            provider_id: claude,
            unit: crate::models::QuotaThresholdUnit::Percentage,
            thresholds: Some(crate::models::QuotaThresholds {
                warning: 70.0,
                critical: 30.0,
                notify: 15.0,
            }),
        }),
    );

    assert_usage_progress(
        &quota_alerts(&refresh_success(&mut session, &codex, 78.0)),
        "Codex",
        78.0,
    );
}

#[test]
fn quota_threshold_equal_override_switch_keeps_other_provider_baseline() {
    let mut session = make_session();
    session.settings.notification.quota_usage_step_pct = 5;
    let claude = pid(ProviderKind::Claude);
    let codex = pid(ProviderKind::Codex);
    enable(&mut session, ProviderKind::Claude);
    enable(&mut session, ProviderKind::Codex);

    refresh_success(&mut session, &codex, 83.0);
    refresh_success(&mut session, &codex, 81.0);

    let defaults = crate::models::QuotaThresholds::DEFAULT_PERCENTAGE;
    reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::SetProviderQuotaThresholds {
            provider_id: claude.clone(),
            unit: crate::models::QuotaThresholdUnit::Percentage,
            thresholds: Some(defaults),
        }),
    );
    reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::SetProviderQuotaThresholds {
            provider_id: claude,
            unit: crate::models::QuotaThresholdUnit::Percentage,
            thresholds: None,
        }),
    );

    assert_usage_progress(
        &quota_alerts(&refresh_success(&mut session, &codex, 78.0)),
        "Codex",
        78.0,
    );
}

#[test]
fn quota_threshold_color_only_change_updates_dynamic_icon() {
    use crate::models::{StatusLevel, TrayIconStyle};

    let mut session = make_session();
    session.settings.display.tray_icon_style = TrayIconStyle::Dynamic;
    let claude = pid(ProviderKind::Claude);
    enable(&mut session, ProviderKind::Claude);

    refresh_success(&mut session, &claude, 30.0);

    let effects = reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::SetGlobalQuotaThresholds {
            unit: crate::models::QuotaThresholdUnit::Percentage,
            thresholds: crate::models::QuotaThresholds {
                warning: 25.0,
                critical: 20.0,
                notify: 10.0,
            },
        }),
    );

    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Context(ContextEffect::ApplyTrayIcon(
            TrayIconRequest::DynamicStatus(StatusLevel::Green)
        ))
    )));
    assert!(quota_alerts(&effects).is_empty());
}

#[test]
fn quota_threshold_color_only_save_preserves_usage_step_baseline() {
    let mut session = make_session();
    session.settings.notification.quota_usage_step_pct = 10;
    let claude = pid(ProviderKind::Claude);
    enable(&mut session, ProviderKind::Claude);

    refresh_success(&mut session, &claude, 90.0);
    refresh_success(&mut session, &claude, 85.0);

    let effects = reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::SetGlobalQuotaThresholds {
            unit: crate::models::QuotaThresholdUnit::Percentage,
            thresholds: crate::models::QuotaThresholds {
                warning: 60.0,
                critical: 20.0,
                notify: 10.0,
            },
        }),
    );
    assert!(quota_alerts(&effects).is_empty());
    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Refresh(RefreshEffect::SendRequest(_)))
    )));

    assert_usage_progress(
        &quota_alerts(&refresh_success(&mut session, &claude, 80.0)),
        "Claude",
        80.0,
    );
    assert!(quota_alerts(&refresh_success(&mut session, &claude, 75.0)).is_empty());
}

#[test]
fn quota_threshold_provider_color_only_save_preserves_usage_step_baseline() {
    let mut session = make_session();
    session.settings.notification.quota_usage_step_pct = 10;
    let claude = pid(ProviderKind::Claude);
    enable(&mut session, ProviderKind::Claude);

    refresh_success(&mut session, &claude, 90.0);
    refresh_success(&mut session, &claude, 85.0);

    reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::SetProviderQuotaThresholds {
            provider_id: claude.clone(),
            unit: crate::models::QuotaThresholdUnit::Percentage,
            thresholds: Some(crate::models::QuotaThresholds {
                warning: 60.0,
                critical: 20.0,
                notify: 10.0,
            }),
        }),
    );

    assert_usage_progress(
        &quota_alerts(&refresh_success(&mut session, &claude, 80.0)),
        "Claude",
        80.0,
    );
    assert!(quota_alerts(&refresh_success(&mut session, &claude, 75.0)).is_empty());
}

#[test]
fn quota_threshold_unrelated_unit_save_preserves_usage_step_baseline() {
    for (unit, thresholds) in [
        (
            crate::models::QuotaThresholdUnit::Currency,
            crate::models::QuotaThresholds {
                warning: 20.0,
                critical: 5.0,
                notify: 2.0,
            },
        ),
        (
            crate::models::QuotaThresholdUnit::Amount,
            crate::models::QuotaThresholds {
                warning: 200.0,
                critical: 50.0,
                notify: 20.0,
            },
        ),
    ] {
        let mut session = make_session();
        session.settings.notification.quota_usage_step_pct = 10;
        let claude = pid(ProviderKind::Claude);
        enable(&mut session, ProviderKind::Claude);

        refresh_success(&mut session, &claude, 90.0);
        refresh_success(&mut session, &claude, 85.0);

        reduce(
            &mut session,
            AppAction::UpdateSetting(SettingChange::SetGlobalQuotaThresholds { unit, thresholds }),
        );

        assert_usage_progress(
            &quota_alerts(&refresh_success(&mut session, &claude, 80.0)),
            "Claude",
            80.0,
        );
        assert!(quota_alerts(&refresh_success(&mut session, &claude, 75.0)).is_empty());
    }
}

#[test]
fn quota_threshold_notify_change_preserves_usage_step_and_rebases_alert() {
    let mut session = make_session();
    session.settings.notification.quota_usage_step_pct = 10;
    let claude = pid(ProviderKind::Claude);
    enable(&mut session, ProviderKind::Claude);

    refresh_success(&mut session, &claude, 40.0);
    refresh_success(&mut session, &claude, 35.0);

    let effects = reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::SetGlobalQuotaThresholds {
            unit: crate::models::QuotaThresholdUnit::Percentage,
            thresholds: crate::models::QuotaThresholds {
                warning: 80.0,
                critical: 40.0,
                notify: 35.0,
            },
        }),
    );
    assert!(quota_alerts(&effects).is_empty());

    let effects = refresh_success(&mut session, &claude, 30.0);
    let alerts = quota_alerts(&effects);
    assert!(
        !alerts
            .iter()
            .any(|a| matches!(a, QuotaNotificationEvent::LowQuota { .. })),
        "rebaseline to Low must not replay LowQuota: {alerts:?}"
    );
    assert_usage_progress(&alerts, "Claude", 30.0);

    let effects = refresh_success(&mut session, &claude, 0.0);
    let alerts = quota_alerts(&effects);
    assert_eq!(alerts.len(), 1, "expected single alert: {alerts:?}");
    assert!(matches!(
        alerts[0],
        QuotaNotificationEvent::Exhausted { .. }
    ));

    assert!(matches!(
        quota_alerts(&refresh_success(&mut session, &claude, 50.0)).first(),
        Some(QuotaNotificationEvent::Recovered { .. })
    ));
}
