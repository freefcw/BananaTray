use std::cell::RefCell;
use std::rc::Rc;

use log::warn;

use crate::application::{AppAction, RefreshEffect};
use crate::models::{ErrorKind, FailureReason, ProviderFailure, ProviderId};
use crate::refresh::{RefreshEvent, RefreshOutcome, RefreshRequest, RefreshResult};

use super::super::AppState;

pub(super) fn run(state: &Rc<RefCell<AppState>>, effect: RefreshEffect) -> Vec<AppAction> {
    match effect {
        RefreshEffect::SendRequest(request) => send_request(state, request),
    }
}

pub(super) fn send_request(
    state: &Rc<RefCell<AppState>>,
    request: RefreshRequest,
) -> Vec<AppAction> {
    let failed_id = match &request {
        RefreshRequest::RefreshOne { id, .. } => Some(id.clone()),
        _ => None,
    };
    let send_result = state.borrow().send_refresh(request);
    if let Err(err) = send_result {
        warn!(target: "refresh", "failed to send refresh request: {}", err);
        failed_id
            .map(|id| vec![refresh_request_send_failed_action(id, err.to_string())])
            .unwrap_or_default()
    } else {
        Vec::new()
    }
}

fn refresh_request_send_failed_action(id: ProviderId, detail: String) -> AppAction {
    AppAction::RefreshEventReceived(RefreshEvent::Finished(RefreshOutcome {
        id,
        result: RefreshResult::Failed {
            failure: ProviderFailure {
                reason: FailureReason::Unavailable,
                advice: None,
                raw_detail: Some(format!("failed to enqueue refresh request: {detail}")),
            },
            error_kind: ErrorKind::Unknown,
        },
    }))
}
