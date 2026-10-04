use crate::application::{AppEffect, ContextEffect};
use crate::history::{
    capture_sample, cutoff_ms, now_ms, query_window, retention_cutoffs, HistoryJob,
    HistoryLoadOutcome, HistoryLoadRequest, HistoryRange,
};
use crate::models::{valid_retention_days, ProviderId};
use crate::refresh::{RefreshReason, RefreshResult};

use super::super::state::{AppSession, HistoryLoadState, SettingsModalState};

pub(super) fn record_adopted_refresh(
    session: &mut AppSession,
    id: &ProviderId,
    reason: Option<RefreshReason>,
    result: &RefreshResult,
    effects: &mut Vec<AppEffect>,
) {
    let captured_at_ms = now_ms();
    let Some(sample) = capture_sample(id, result, reason, captured_at_ms) else {
        return;
    };
    let days = session.settings.effective_history_retention_days(id);
    effects.push(
        HistoryJob::Append {
            sample,
            cutoff_ms: cutoff_ms(captured_at_ms, days),
        }
        .into(),
    );
    if &session.settings_ui.selected_provider == id {
        begin_history_load(session, effects);
    }
}

pub(super) fn begin_history_load(session: &mut AppSession, effects: &mut Vec<AppEffect>) {
    let provider_id = session.settings_ui.selected_provider.clone();
    session.history_ui.request_id = session.history_ui.request_id.wrapping_add(1);
    let request_id = session.history_ui.request_id;
    let days = session
        .settings
        .effective_history_retention_days(&provider_id);
    let (axis_start_ms, axis_end_ms, captured_from_ms, captured_to_ms) =
        query_window(session.history_ui.range, now_ms(), days);
    session.history_ui.load = HistoryLoadState::Loading {
        provider_id: provider_id.clone(),
        request_id,
    };
    effects.push(
        HistoryJob::Load(HistoryLoadRequest {
            request_id,
            provider_id: provider_id.id_key(),
            axis_start_ms,
            axis_end_ms,
            captured_from_ms,
            // 查询是左闭右开。窗口终点那一毫秒的样本也要能读到。
            captured_to_ms: captured_to_ms.saturating_add(1),
            range: session.history_ui.range,
        })
        .into(),
    );
    effects.push(ContextEffect::Render.into());
}

pub(super) fn push_apply_retention(session: &AppSession, effects: &mut Vec<AppEffect>) {
    let loaded = session.provider_store.custom_provider_ids();
    let targets = retention_cutoffs(
        &session.settings.provider.provider_layout,
        &loaded,
        &session.settings,
        now_ms(),
    );
    effects.push(HistoryJob::ApplyRetention { targets }.into());
}

pub(super) fn apply_loaded(
    session: &mut AppSession,
    request_id: u64,
    provider_id: ProviderId,
    outcome: HistoryLoadOutcome,
    effects: &mut Vec<AppEffect>,
) {
    let current = matches!(
        &session.history_ui.load,
        HistoryLoadState::Loading {
            provider_id: expected,
            request_id: expected_id,
        } if expected == &provider_id && *expected_id == request_id
    );
    if !current {
        return;
    }
    session.history_ui.load = match outcome {
        HistoryLoadOutcome::Ready(ready) => HistoryLoadState::Ready {
            provider_id,
            request_id,
            ready,
        },
        HistoryLoadOutcome::Unavailable => HistoryLoadState::Unavailable {
            provider_id,
            request_id,
        },
    };
    effects.push(ContextEffect::Render.into());
}

pub(super) fn set_history_range(
    session: &mut AppSession,
    range: HistoryRange,
    effects: &mut Vec<AppEffect>,
) {
    session.history_ui.range = range;
    begin_history_load(session, effects);
}

pub(super) fn toggle_history_retention_dropdown(
    session: &mut AppSession,
    effects: &mut Vec<AppEffect>,
) {
    session.settings_ui.history_retention_dropdown_open =
        !session.settings_ui.history_retention_dropdown_open;
    if session.settings_ui.history_retention_dropdown_open {
        session.settings_ui.cadence_dropdown_open = false;
    }
    effects.push(ContextEffect::Render.into());
}

pub(super) fn toggle_provider_history_retention_dropdown(
    session: &mut AppSession,
    effects: &mut Vec<AppEffect>,
) {
    session.settings_ui.provider_history_retention_dropdown_open =
        !session.settings_ui.provider_history_retention_dropdown_open;
    effects.push(ContextEffect::Render.into());
}

pub(super) fn begin_clear_provider_history(session: &mut AppSession, effects: &mut Vec<AppEffect>) {
    if !matches!(
        session.settings_ui.modal,
        SettingsModalState::Idle | SettingsModalState::ConfirmingClearProviderHistory
    ) {
        return;
    }
    session.settings_ui.modal = SettingsModalState::ConfirmingClearProviderHistory;
    effects.push(ContextEffect::Render.into());
}

pub(super) fn cancel_clear_provider_history(
    session: &mut AppSession,
    effects: &mut Vec<AppEffect>,
) {
    if session
        .settings_ui
        .modal
        .is_confirming_clear_provider_history()
    {
        session.settings_ui.modal = SettingsModalState::Idle;
        effects.push(ContextEffect::Render.into());
    }
}

pub(super) fn confirm_clear_provider_history(
    session: &mut AppSession,
    effects: &mut Vec<AppEffect>,
) {
    if !session
        .settings_ui
        .modal
        .is_confirming_clear_provider_history()
    {
        return;
    }
    let provider_id = session.settings_ui.selected_provider.id_key();
    session.settings_ui.modal = SettingsModalState::Idle;
    effects.push(HistoryJob::PurgeProvider { provider_id }.into());
    begin_history_load(session, effects);
}

pub(super) fn begin_clear_all_history(session: &mut AppSession, effects: &mut Vec<AppEffect>) {
    if !matches!(
        session.settings_ui.modal,
        SettingsModalState::Idle | SettingsModalState::ConfirmingClearAllHistory
    ) {
        return;
    }
    session.settings_ui.modal = SettingsModalState::ConfirmingClearAllHistory;
    effects.push(ContextEffect::Render.into());
}

pub(super) fn cancel_clear_all_history(session: &mut AppSession, effects: &mut Vec<AppEffect>) {
    if session.settings_ui.modal.is_confirming_clear_all_history() {
        session.settings_ui.modal = SettingsModalState::Idle;
        effects.push(ContextEffect::Render.into());
    }
}

pub(super) fn confirm_clear_all_history(session: &mut AppSession, effects: &mut Vec<AppEffect>) {
    if !session.settings_ui.modal.is_confirming_clear_all_history() {
        return;
    }
    session.settings_ui.modal = SettingsModalState::Idle;
    effects.push(HistoryJob::PurgeAll.into());
    begin_history_load(session, effects);
}

pub(super) fn set_global_retention(
    session: &mut AppSession,
    days: u16,
    effects: &mut Vec<AppEffect>,
) -> bool {
    if !valid_retention_days(days) {
        return false;
    }
    session.settings.history.retention_days = days;
    session.settings_ui.history_retention_dropdown_open = false;
    push_apply_retention(session, effects);
    begin_history_load(session, effects);
    true
}

pub(super) fn set_provider_retention(
    session: &mut AppSession,
    provider_id: ProviderId,
    days: Option<u16>,
    effects: &mut Vec<AppEffect>,
) -> bool {
    if days.is_some_and(|days| !valid_retention_days(days)) {
        return false;
    }
    session
        .settings
        .provider
        .set_history_retention_days(&provider_id, days);
    session.settings_ui.provider_history_retention_dropdown_open = false;
    push_apply_retention(session, effects);
    if session.settings_ui.selected_provider == provider_id {
        begin_history_load(session, effects);
    }
    true
}

pub(super) fn note_deleted_provider(
    session: &mut AppSession,
    provider_id: &ProviderId,
    effects: &mut Vec<AppEffect>,
) {
    effects.push(
        HistoryJob::PurgeProvider {
            provider_id: provider_id.id_key(),
        }
        .into(),
    );
    begin_history_load(session, effects);
}
