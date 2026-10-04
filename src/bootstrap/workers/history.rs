use crate::history::{HistoryJob, QuotaHistoryStore, SqliteQuotaHistoryStore};
use crate::runtime::AppState;
use gpui::App;
use std::cell::RefCell;
use std::rc::Rc;

use crate::bootstrap::capabilities::dispatch_in_app;

pub(crate) type HistorySender = crate::runtime::PersistentJobSender<HistoryJob>;
pub(crate) type HistoryReceiver = crate::runtime::PersistentJobReceiver<HistoryJob>;

/// 启动用量历史线程。数据库打不开时线程仍在，但只对读取返回不可用。
pub(crate) fn start_history_pump(state: &Rc<RefCell<AppState>>, rx: HistoryReceiver, cx: &mut App) {
    let (wake_tx, wake_rx) = smol::channel::unbounded::<()>();
    let results = state.borrow().history_results.clone();
    let path = crate::platform::paths::quota_history_path();

    let owner = crate::utils::BoundedThreadOwner::spawn("quota-history", move || {
        let mut store: Option<Box<dyn QuotaHistoryStore>> =
            match SqliteQuotaHistoryStore::open(&path) {
                Ok(store) => Some(Box::new(store)),
                Err(err) => {
                    log::warn!(target: "history", "quota history database unavailable: {err}");
                    None
                }
            };
        while let Some(job) = rx.recv() {
            if let Some(action) = crate::runtime::execute_history_job(&mut store, job) {
                results.push(action);
            }
            let _ = wake_tx.try_send(());
        }
    })
    .expect("failed to spawn quota history thread");
    state.borrow().history_tx.attach_owner(owner);

    let state = state.clone();
    let pump_cx = cx.to_async();
    cx.to_async()
        .foreground_executor()
        .spawn(async move {
            while wake_rx.recv().await.is_ok() {
                let _ = pump_cx.update(|cx| {
                    let actions = state.borrow().history_results.drain();
                    for action in actions {
                        dispatch_in_app(&state, action, cx);
                    }
                });
            }
        })
        .detach();
}
