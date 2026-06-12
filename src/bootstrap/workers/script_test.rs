use crate::application::AppAction;
use crate::models::ScriptProviderConfig;
use crate::runtime::AppState;
use gpui::App;
use std::cell::RefCell;
use std::rc::Rc;

use crate::bootstrap::capabilities::dispatch_in_app;

type ScriptTestRequest = (u64, ScriptProviderConfig);
pub(crate) type ScriptTestSender = smol::channel::Sender<ScriptTestRequest>;
pub(crate) type ScriptTestReceiver = smol::channel::Receiver<ScriptTestRequest>;

pub(crate) fn script_test_channel() -> (ScriptTestSender, ScriptTestReceiver) {
    smol::channel::bounded(8)
}

/// 启动脚本测试事件泵：后台执行脚本，完成后回到 UI 线程回填结果。
pub(crate) fn start_script_test_pump(
    state: &Rc<RefCell<AppState>>,
    script_test_rx: ScriptTestReceiver,
    cx: &mut App,
) {
    let (result_tx, result_rx) =
        smol::channel::bounded::<(u64, crate::models::ScriptProviderTestResult)>(8);

    std::thread::Builder::new()
        .name("script-provider-test".into())
        .spawn(move || {
            while let Ok((request_id, config)) = smol::block_on(script_test_rx.recv()) {
                let result = crate::runtime::execute_script_provider_test(&config);
                if smol::block_on(result_tx.send((request_id, result))).is_err() {
                    break;
                }
            }
        })
        .expect("failed to spawn script provider test thread");

    let state = state.clone();
    let pump_cx = cx.to_async();
    cx.to_async()
        .foreground_executor()
        .spawn(async move {
            while let Ok((request_id, result)) = result_rx.recv().await {
                let _ = pump_cx.update(|cx| {
                    dispatch_in_app(
                        &state,
                        AppAction::ScriptProviderTestFinished { request_id, result },
                        cx,
                    );
                });
            }
        })
        .detach();
}
