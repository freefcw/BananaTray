use std::cell::RefCell;
use std::rc::Rc;

use log::warn;

use crate::application::AppAction;
use crate::history::{
    interpret, HistoryJob, HistoryLoadOutcome, HistoryLoadRequest, HistoryRangeQuery,
    QuotaHistoryStore,
};
use crate::models::ProviderId;

use super::super::AppState;

pub(super) fn run(state: &Rc<RefCell<AppState>>, job: HistoryJob) -> Vec<AppAction> {
    match state.borrow().history_tx.try_send(job) {
        Ok(()) => Vec::new(),
        Err(err) => {
            warn!(target: "history", "history worker unavailable, job dropped");
            match err.into_inner() {
                HistoryJob::Load(request) => vec![unavailable_action(request)],
                _ => Vec::new(),
            }
        }
    }
}

pub(crate) fn execute_job(
    store: &mut Option<Box<dyn QuotaHistoryStore>>,
    job: HistoryJob,
) -> Option<AppAction> {
    let Some(store) = store.as_mut() else {
        return match job {
            HistoryJob::Load(request) => {
                warn!(target: "history", "quota history is unavailable; load reports unavailable");
                Some(unavailable_action(request))
            }
            HistoryJob::Append { sample, .. } => {
                warn!(
                    target: "history",
                    "quota history is unavailable; dropping sample for {}",
                    sample.provider_id
                );
                None
            }
            HistoryJob::PurgeProvider { provider_id } => {
                warn!(
                    target: "history",
                    "quota history is unavailable; dropping purge for {provider_id}"
                );
                None
            }
            HistoryJob::PurgeAll => {
                warn!(target: "history", "quota history is unavailable; dropping purge all");
                None
            }
            HistoryJob::ApplyRetention { .. } => {
                warn!(target: "history", "quota history is unavailable; dropping retention");
                None
            }
        };
    };

    match job {
        HistoryJob::Append { sample, cutoff_ms } => {
            if let Err(err) = store.append(&sample) {
                warn!(
                    target: "history",
                    "failed to record quota history for {}: {err}",
                    sample.provider_id
                );
                return None;
            }
            if let Err(err) = store.purge_provider_before(&sample.provider_id, cutoff_ms) {
                warn!(
                    target: "history",
                    "failed to apply retention for {}: {err}",
                    sample.provider_id
                );
            }
            None
        }
        HistoryJob::Load(request) => {
            let query = HistoryRangeQuery {
                provider_id: request.provider_id.clone(),
                captured_from_ms: request.captured_from_ms,
                captured_to_ms: request.captured_to_ms,
                include_non_success: true,
            };
            match store.load_rows(&query) {
                Ok(rows) => Some(ready_action(request, rows)),
                Err(err) => {
                    warn!(target: "history", "failed to load quota history: {err}");
                    Some(unavailable_action(request))
                }
            }
        }
        HistoryJob::PurgeProvider { provider_id } => {
            if let Err(err) = store.purge_provider(&provider_id) {
                warn!(target: "history", "failed to clear history for {provider_id}: {err}");
            }
            None
        }
        HistoryJob::PurgeAll => {
            if let Err(err) = store.purge_all() {
                warn!(target: "history", "failed to clear all quota history: {err}");
            }
            None
        }
        HistoryJob::ApplyRetention { targets } => {
            for target in targets {
                if let Err(err) = store.purge_provider_before(&target.provider_id, target.cutoff_ms)
                {
                    warn!(
                        target: "history",
                        "failed to apply retention for {}: {err}",
                        target.provider_id
                    );
                }
            }
            None
        }
    }
}

fn unavailable_action(request: HistoryLoadRequest) -> AppAction {
    AppAction::QuotaHistoryLoaded {
        request_id: request.request_id,
        provider_id: ProviderId::from_id_key(&request.provider_id),
        outcome: HistoryLoadOutcome::Unavailable,
    }
}

fn ready_action(request: HistoryLoadRequest, rows: Vec<crate::history::HistoryRow>) -> AppAction {
    let outcome = HistoryLoadOutcome::Ready(interpret(
        &rows,
        request.range,
        request.axis_start_ms,
        request.axis_end_ms,
    ));
    AppAction::QuotaHistoryLoaded {
        request_id: request.request_id,
        provider_id: ProviderId::from_id_key(&request.provider_id),
        outcome,
    }
}
