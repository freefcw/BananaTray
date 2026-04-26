use super::common::{has_effect, has_render, make_custom_provider_status, make_session, pid};
use crate::application::{
    reduce, AppAction, AppEffect, CommonEffect, ContextEffect, RefreshEffect, SettingsEffect,
    TrayIconRequest,
};
use crate::models::test_helpers::make_test_provider;
use crate::models::{ConnectionStatus, NavTab, ProviderId, ProviderKind, RefreshData};
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
fn refresh_non_current_provider_does_not_produce_tray_icon_effect() {
    use crate::models::{QuotaInfo, TrayIconStyle};

    let mut session = make_session();
    session.settings.display.tray_icon_style = TrayIconStyle::Dynamic;
    // 当前 Provider 是 Claude（默认），但刷新的是 Gemini

    let effects = reduce(
        &mut session,
        AppAction::RefreshEventReceived(RefreshEvent::Finished(RefreshOutcome {
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

    // 非当前 Provider 的刷新不影响图标
    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Context(ContextEffect::ApplyTrayIcon(_))
    )));
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
