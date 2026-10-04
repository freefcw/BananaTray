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
