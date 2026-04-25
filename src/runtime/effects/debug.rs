use std::cell::RefCell;
use std::rc::Rc;

use log::{info, warn};

use crate::application::DebugEffect;
use crate::refresh::{RefreshReason, RefreshRequest};
use crate::utils::log_capture::LogCapture;

use super::super::AppState;

pub(super) fn run(state: &Rc<RefCell<AppState>>, effect: DebugEffect) {
    match effect {
        DebugEffect::OpenLogDirectory => {
            let log_path = state.borrow().log_path.clone();
            if let Some(path) = log_path {
                crate::platform::system::open_path_in_finder(&path);
            } else {
                warn!(target: "runtime", "OpenLogDirectory: log_path not available");
            }
        }
        DebugEffect::CopyToClipboard(text) => {
            crate::platform::system::copy_to_clipboard(&text);
        }
        DebugEffect::StartRefresh(kind) => {
            info!(target: "runtime", "starting debug refresh for {:?}", kind);
            // 保存当前日志级别到 state，供 RestoreLogLevel 使用。
            state.borrow_mut().session.debug_ui.prev_log_level = Some(log::max_level());
            LogCapture::global().clear();
            LogCapture::global().enable();
            log::set_max_level(log::LevelFilter::Debug);
            let request = RefreshRequest::RefreshOne {
                id: kind,
                reason: RefreshReason::Manual,
            };
            let _ = super::refresh::send_request(state, request);
        }
        DebugEffect::RestoreLogLevel(level) => {
            info!(target: "runtime", "debug refresh complete, restoring log level to {:?}", level);
            LogCapture::global().disable();
            log::set_max_level(level);
        }
        DebugEffect::ClearLogs => {
            LogCapture::global().clear();
        }
    }
}
