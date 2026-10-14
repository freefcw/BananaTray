use super::common::{has_effect, make_session, pid};
use crate::application::state::HistoryLoadState;
use crate::application::{
    reduce, AppAction, AppEffect, CommonEffect, SettingChange, SettingsModalState, SettingsTab,
};
use crate::history::{HistoryJob, HistoryLoadOutcome, HistoryRange};
use crate::models::{NavTab, ProviderKind, QuotaInfo, RefreshData};
use crate::refresh::{RefreshEvent, RefreshOutcome, RefreshReason, RefreshResult};

fn finished(result: RefreshResult) -> AppAction {
    AppAction::RefreshEventReceived(RefreshEvent::Finished(RefreshOutcome {
        reason: Some(RefreshReason::Manual),
        id: pid(ProviderKind::Claude),
        result,
    }))
}

fn success() -> RefreshResult {
    RefreshResult::Success {
        data: RefreshData {
            quotas: vec![QuotaInfo::new("session", 10.0, 100.0)],
            account_email: None,
            account_tier: None,
            source_label: None,
            rate_limit_resets: Default::default(),
        },
    }
}

fn appends_claude(effects: &[AppEffect]) -> bool {
    effects.iter().any(|effect| {
        matches!(
            effect,
            AppEffect::Common(CommonEffect::History(HistoryJob::Append { sample, .. }))
                if sample.provider_id == "claude"
        )
    })
}

#[test]
fn adopted_success_is_recorded_and_a_skip_is_not() {
    let mut session = make_session();
    session.settings_ui.selected_provider = pid(ProviderKind::Claude);
    // 默认布局里 Claude 在侧栏但不开启后台监控，未启用的结果不会落库。
    session
        .settings
        .provider
        .set_enabled(&pid(ProviderKind::Claude), true);
    let effects = reduce(&mut session, finished(success()));
    assert!(appends_claude(&effects));

    let mut session = make_session();
    let effects = reduce(&mut session, finished(RefreshResult::SkippedCooldown));
    assert!(!has_effect(&effects, |effect| {
        matches!(effect, AppEffect::Common(CommonEffect::History(_)))
    }));
}

#[test]
fn disabled_provider_result_is_not_recorded() {
    let mut session = make_session();
    session
        .settings
        .provider
        .set_enabled(&pid(ProviderKind::Claude), false);
    let effects = reduce(&mut session, finished(success()));
    assert!(!appends_claude(&effects));
}

#[test]
fn chart_query_starts_at_the_retention_cutoff() {
    let mut session = make_session();
    session.settings_ui.selected_provider = pid(ProviderKind::Claude);
    session.history_ui.range = HistoryRange::Last30Days;
    let effects = reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::SetHistoryRetentionDays(7)),
    );
    let request = effects.iter().find_map(|effect| match effect {
        AppEffect::Common(CommonEffect::History(HistoryJob::Load(request))) => Some(request),
        _ => None,
    });
    let request = request.expect("retention change reloads the chart");
    assert_eq!(request.provider_id, "claude");
    assert!(request.captured_from_ms > request.axis_start_ms);
    assert_eq!(
        request.captured_to_ms,
        request.axis_end_ms.saturating_add(1)
    );
    assert_eq!(session.settings.history.retention_days, 7);
}

#[test]
fn switching_settings_tab_closes_history_clear_confirmation() {
    let mut session = make_session();
    session.settings_ui.modal = SettingsModalState::ConfirmingClearAllHistory;
    reduce(
        &mut session,
        AppAction::SetSettingsTab(SettingsTab::Providers),
    );
    assert_eq!(session.settings_ui.modal, SettingsModalState::Idle);

    session.settings_ui.modal = SettingsModalState::ConfirmingClearProviderHistory;
    reduce(
        &mut session,
        AppAction::SetSettingsTab(SettingsTab::General),
    );
    assert_eq!(session.settings_ui.modal, SettingsModalState::Idle);
}

#[test]
fn invalid_retention_days_are_ignored() {
    let mut session = make_session();
    let effects = reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::SetHistoryRetentionDays(0)),
    );
    assert!(effects.is_empty());
    assert_eq!(session.settings.history.retention_days, 90);
}

#[test]
fn clear_all_requires_confirmation_and_does_not_clobber_a_form() {
    let mut session = make_session();
    session.settings_ui.modal = SettingsModalState::AddingNewApi;
    reduce(&mut session, AppAction::BeginClearAllHistory);
    assert_eq!(session.settings_ui.modal, SettingsModalState::AddingNewApi);

    session.settings_ui.modal = SettingsModalState::Idle;
    reduce(&mut session, AppAction::ConfirmClearAllHistory);
    assert_eq!(session.settings_ui.modal, SettingsModalState::Idle);

    reduce(&mut session, AppAction::BeginClearAllHistory);
    let effects = reduce(&mut session, AppAction::ConfirmClearAllHistory);
    assert!(has_effect(&effects, |effect| {
        matches!(
            effect,
            AppEffect::Common(CommonEffect::History(HistoryJob::PurgeAll))
        )
    }));
    assert!(has_effect(&effects, |effect| {
        matches!(
            effect,
            AppEffect::Common(CommonEffect::History(HistoryJob::Load(_)))
        )
    }));
    assert_eq!(session.settings_ui.modal, SettingsModalState::Idle);
}

#[test]
fn startup_retention_includes_builtins_only_from_the_loaded_set() {
    let mut session = make_session();
    session
        .settings
        .provider
        .provider_layout
        .push(crate::models::ProviderLayoutItem::new(
            "relay:script",
            true,
            false,
        ));
    let effects = reduce(&mut session, AppAction::ApplyHistoryRetention);
    let targets = effects.iter().find_map(|effect| match effect {
        AppEffect::Common(CommonEffect::History(HistoryJob::ApplyRetention { targets })) => {
            Some(targets)
        }
        _ => None,
    });
    let targets = targets.expect("startup applies retention");
    assert!(targets.iter().any(|target| target.provider_id == "claude"));
    assert!(targets
        .iter()
        .all(|target| target.provider_id != "relay:script"));
}

#[test]
fn switching_to_provider_tab_loads_popup_history_but_overview_does_not() {
    let mut session = make_session();
    let effects = reduce(
        &mut session,
        AppAction::SelectNavTab(NavTab::Provider(pid(ProviderKind::Claude))),
    );
    let request = effects.iter().find_map(|effect| match effect {
        AppEffect::Common(CommonEffect::History(HistoryJob::Load(request))) => Some(request),
        _ => None,
    });
    let request = request.expect("provider tab loads popup history");
    assert_eq!(request.provider_id, "claude");
    assert_eq!(request.range, HistoryRange::Last24Hours);
    assert!(matches!(
        session.popup_history_ui.load,
        HistoryLoadState::Loading { .. }
    ));

    let effects = reduce(&mut session, AppAction::SelectNavTab(NavTab::Overview));
    assert!(!has_effect(&effects, |effect| {
        matches!(
            effect,
            AppEffect::Common(CommonEffect::History(HistoryJob::Load(_)))
        )
    }));
}

#[test]
fn opening_popup_with_provider_tab_loads_popup_history() {
    let mut session = make_session();
    session
        .nav
        .switch_to(NavTab::Provider(pid(ProviderKind::Claude)));
    let effects = reduce(&mut session, AppAction::PopupVisibilityChanged(true));
    assert!(has_effect(&effects, |effect| {
        matches!(
            effect,
            AppEffect::Common(CommonEffect::History(HistoryJob::Load(request)))
                if request.provider_id == "claude"
        )
    }));
}

#[test]
fn adopted_refresh_reloads_the_active_popup_provider_chart() {
    let mut session = make_session();
    session
        .nav
        .switch_to(NavTab::Provider(pid(ProviderKind::Claude)));
    session
        .settings
        .provider
        .set_enabled(&pid(ProviderKind::Claude), true);
    let effects = reduce(&mut session, finished(success()));
    assert!(appends_claude(&effects));
    assert!(has_effect(&effects, |effect| {
        matches!(
            effect,
            AppEffect::Common(CommonEffect::History(HistoryJob::Load(request)))
                if request.provider_id == "claude"
        )
    }));
}

#[test]
fn load_result_settles_popup_scope_without_touching_settings_scope() {
    let mut session = make_session();
    session.popup_history_ui.request_id = 7;
    session.popup_history_ui.load = HistoryLoadState::Loading {
        provider_id: pid(ProviderKind::Claude),
        request_id: 7,
    };
    let ready = crate::history::HistoryReady {
        range: HistoryRange::Last24Hours,
        axis_start_ms: 0,
        axis_end_ms: 1,
        state: crate::history::HistoryReadyState::Empty,
    };
    reduce(
        &mut session,
        AppAction::QuotaHistoryLoaded {
            scope: crate::history::HistoryScope::Popup,
            request_id: 7,
            provider_id: pid(ProviderKind::Claude),
            outcome: HistoryLoadOutcome::Ready(ready),
        },
    );
    assert!(matches!(
        session.popup_history_ui.load,
        HistoryLoadState::Ready { .. }
    ));
    assert!(matches!(session.history_ui.load, HistoryLoadState::Idle));
}

#[test]
fn fixed_reduction_time_drives_samples_queries_and_refresh_age() {
    let mut session = make_session();
    let id = pid(ProviderKind::Claude);
    session.settings_ui.selected_provider = id.clone();
    session.nav.switch_to(NavTab::Provider(id.clone()));
    session.settings.provider.set_enabled(&id, true);
    let time = crate::application::ReductionTime {
        wall_ms: 1_900_000_000_123,
        monotonic: std::time::Instant::now() - std::time::Duration::from_secs(17),
    };
    let effects = crate::application::reduce_at(&mut session, finished(success()), time);
    let mut loads = 0;
    for effect in effects {
        match effect {
            AppEffect::Common(CommonEffect::History(HistoryJob::Append { sample, cutoff_ms })) => {
                assert_eq!(sample.captured_at_ms, time.wall_ms);
                assert_eq!(cutoff_ms, time.wall_ms - 90 * crate::history::DAY_MS);
            }
            AppEffect::Common(CommonEffect::History(HistoryJob::Load(request))) => {
                assert_eq!(request.axis_end_ms, time.wall_ms);
                assert_eq!(request.captured_to_ms, time.wall_ms + 1);
                loads += 1;
            }
            _ => {}
        }
    }
    assert_eq!(loads, 2);
    assert_eq!(
        session
            .provider_store
            .find_by_id(&id)
            .unwrap()
            .last_refreshed_instant,
        Some(time.monotonic)
    );
}

#[test]
fn same_numbered_history_queries_do_not_cross_scopes() {
    let mut session = make_session();
    session.settings_ui.selected_provider = pid(ProviderKind::Claude);
    let settings_effects = reduce(
        &mut session,
        AppAction::SetHistoryRange(HistoryRange::Last7Days),
    );
    let popup_effects = reduce(
        &mut session,
        AppAction::SelectNavTab(NavTab::Provider(pid(ProviderKind::Claude))),
    );
    let load_request = |effects: Vec<AppEffect>| {
        effects
            .into_iter()
            .find_map(|effect| match effect {
                AppEffect::Common(CommonEffect::History(HistoryJob::Load(request))) => {
                    Some(request)
                }
                _ => None,
            })
            .unwrap()
    };
    let settings_request = load_request(settings_effects);
    let popup_request = load_request(popup_effects);
    assert_eq!(settings_request.request_id, popup_request.request_id);
    reduce(
        &mut session,
        AppAction::QuotaHistoryLoaded {
            scope: settings_request.scope,
            request_id: settings_request.request_id,
            provider_id: pid(ProviderKind::Claude),
            outcome: HistoryLoadOutcome::Ready(crate::history::HistoryReady {
                range: settings_request.range,
                axis_start_ms: settings_request.axis_start_ms,
                axis_end_ms: settings_request.axis_end_ms,
                state: crate::history::HistoryReadyState::Empty,
            }),
        },
    );
    assert!(matches!(
        session.history_ui.load,
        HistoryLoadState::Ready { .. }
    ));
    assert!(matches!(
        session.popup_history_ui.load,
        HistoryLoadState::Loading { .. }
    ));
    reduce(
        &mut session,
        AppAction::QuotaHistoryLoaded {
            scope: popup_request.scope,
            request_id: popup_request.request_id,
            provider_id: pid(ProviderKind::Claude),
            outcome: HistoryLoadOutcome::Ready(crate::history::HistoryReady {
                range: popup_request.range,
                axis_start_ms: popup_request.axis_start_ms,
                axis_end_ms: popup_request.axis_end_ms,
                state: crate::history::HistoryReadyState::Empty,
            }),
        },
    );
    assert!(
        matches!(session.history_ui.load, HistoryLoadState::Ready { ready, .. } if ready.range == HistoryRange::Last7Days)
    );
    assert!(
        matches!(session.popup_history_ui.load, HistoryLoadState::Ready { ready, .. } if ready.range == HistoryRange::Last24Hours)
    );
}

#[test]
fn stale_history_response_is_ignored_within_its_scope() {
    let mut session = make_session();
    session.settings_ui.selected_provider = pid(ProviderKind::Claude);
    reduce(
        &mut session,
        AppAction::SetHistoryRange(HistoryRange::Last7Days),
    );
    let old_id = session.history_ui.request_id;
    reduce(
        &mut session,
        AppAction::SetHistoryRange(HistoryRange::Last30Days),
    );
    let effects = reduce(
        &mut session,
        AppAction::QuotaHistoryLoaded {
            scope: crate::history::HistoryScope::Settings,
            request_id: old_id,
            provider_id: pid(ProviderKind::Claude),
            outcome: HistoryLoadOutcome::Unavailable,
        },
    );
    assert!(effects.is_empty());
    assert!(
        matches!(session.history_ui.load, HistoryLoadState::Loading { request_id, .. } if request_id != old_id)
    );
}
