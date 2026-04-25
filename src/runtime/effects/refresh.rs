use std::cell::RefCell;
use std::rc::Rc;

use log::warn;

use crate::application::RefreshEffect;
use crate::models::ConnectionStatus;
use crate::refresh::RefreshRequest;

use super::super::AppState;

pub(super) fn run(state: &Rc<RefCell<AppState>>, effect: RefreshEffect) {
    match effect {
        RefreshEffect::SendRequest(request) => {
            let _ = send_request(state, request);
        }
    }
}

pub(super) fn send_request(state: &Rc<RefCell<AppState>>, request: RefreshRequest) -> bool {
    let failed_id = match &request {
        RefreshRequest::RefreshOne { id, .. } => Some(id.clone()),
        _ => None,
    };
    let send_result = state.borrow().send_refresh(request);
    if let Err(err) = send_result {
        warn!(target: "refresh", "failed to send refresh request: {}", err);
        if let Some(ref id) = failed_id {
            if let Some(provider) = state.borrow_mut().session.provider_store.find_by_id_mut(id) {
                provider.connection = ConnectionStatus::Disconnected;
            }
        }
        false
    } else {
        true
    }
}
