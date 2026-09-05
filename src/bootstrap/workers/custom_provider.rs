use crate::runtime::AppState;
use gpui::App;
use std::cell::RefCell;
use std::rc::Rc;

use crate::bootstrap::capabilities::dispatch_in_app;

pub(crate) type CustomProviderSender =
    crate::runtime::PersistentJobSender<crate::runtime::CustomProviderJob>;
pub(crate) type CustomProviderReceiver =
    crate::runtime::PersistentJobReceiver<crate::runtime::CustomProviderJob>;

pub(crate) fn custom_provider_channel() -> (CustomProviderSender, CustomProviderReceiver) {
    crate::runtime::PersistentJobSender::channel(8)
}

/// 启动 custom-provider CRUD 阻塞工作线程，完成后回到同一前台 reducer。
pub(crate) fn start_custom_provider_pump(
    state: &Rc<RefCell<AppState>>,
    custom_provider_rx: CustomProviderReceiver,
    cx: &mut App,
) {
    let (result_tx, result_rx) = smol::channel::unbounded::<()>();
    let results = state.borrow().custom_provider_results.clone();
    let settings_writer = state
        .borrow()
        .settings_writer
        .handle()
        .expect("settings writer must be running before background worker startup");

    let owner = crate::utils::BoundedThreadOwner::spawn("custom-provider-io-worker", move || {
        while let Some(job) = custom_provider_rx.recv() {
            let action = crate::runtime::execute_custom_provider_job(job, &settings_writer);
            results.push(action);
            // 前台 pump 若已结束，事务仍必须继续 drain；结果保留在可靠 ledger 中，
            // 退出结算会同步 reduce。
            let _ = result_tx.try_send(());
        }
    })
    .expect("failed to spawn custom-provider I/O thread");
    state.borrow().custom_provider_tx.attach_owner(owner);

    let state = state.clone();
    let pump_cx = cx.to_async();
    cx.to_async()
        .foreground_executor()
        .spawn(async move {
            while result_rx.recv().await.is_ok() {
                let _ = pump_cx.update(|cx| {
                    let actions = state.borrow().custom_provider_results.drain();
                    for action in actions {
                        dispatch_in_app(&state, action, cx);
                    }
                });
            }
        })
        .detach();
}
