use crate::application::AppAction;
use crate::runtime::AppState;
use gpui::App;
use std::cell::RefCell;
use std::rc::Rc;

use crate::bootstrap::capabilities::dispatch_in_app;

pub(crate) type ScriptTestSender =
    crate::runtime::BackgroundJobSender<crate::runtime::ScriptTestJob>;
pub(crate) type ScriptTestReceiver =
    crate::runtime::BackgroundJobReceiver<crate::runtime::ScriptTestJob>;

pub(crate) fn script_test_channel() -> (ScriptTestSender, ScriptTestReceiver) {
    crate::runtime::BackgroundJobSender::channel(8)
}

/// 启动脚本测试事件泵：后台执行脚本，完成后回到 UI 线程回填结果。
pub(crate) fn start_script_test_pump(
    state: &Rc<RefCell<AppState>>,
    script_test_rx: ScriptTestReceiver,
    cx: &mut App,
) {
    let (result_tx, result_rx) = smol::channel::unbounded::<AppAction>();
    let owner = crate::utils::BoundedThreadOwner::spawn("script-test-worker", move || {
        while let Some(job) = script_test_rx.recv() {
            let action = crate::runtime::execute_script_test_job(job);
            if result_tx.try_send(action).is_err() {
                break;
            }
        }
    })
    .expect("failed to spawn script provider test thread");
    state.borrow().script_test_tx.attach_owner(owner);

    let state = state.clone();
    let pump_cx = cx.to_async();
    cx.to_async()
        .foreground_executor()
        .spawn(async move {
            while let Ok(action) = result_rx.recv().await {
                let _ = pump_cx.update(|cx| {
                    dispatch_in_app(&state, action, cx);
                });
            }
        })
        .detach();
}
