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
    let failed_ids = match &request {
        RefreshRequest::RefreshAll { ids, .. } => ids.clone(),
        RefreshRequest::RefreshOne { id, .. } => vec![id.clone()],
        _ => Vec::new(),
    };
    let send_result = state.borrow().send_refresh(request);
    if let Err(err) = send_result {
        // 请求通道为 unbounded：发送失败仅发生在协调器线程终止后。
        // 对所有乐观标记为 Refreshing 的目标生成完成事件，避免 UI 永久卡住。
        warn!(target: "refresh", "refresh coordinator unavailable, request dropped: {}", err);
        let detail = err.to_string();
        failed_ids
            .into_iter()
            .map(|id| refresh_request_send_failed_action(id, detail.clone()))
            .collect()
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
                raw_detail: Some(format!("refresh coordinator unavailable: {detail}")),
            },
            error_kind: ErrorKind::Unknown,
        },
    }))
}
