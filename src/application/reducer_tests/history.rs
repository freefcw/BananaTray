use super::common::{has_effect, make_session, pid};
use crate::application::{
    reduce, AppAction, AppEffect, CommonEffect, SettingChange, SettingsModalState,
};
use crate::history::{HistoryJob, HistoryRange};
use crate::models::{ProviderKind, QuotaInfo, RefreshData};
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
