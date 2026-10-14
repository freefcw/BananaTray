use std::cell::RefCell;
use std::rc::Rc;

use crate::runtime::AppState;
use gpui::App;

pub(crate) fn start_token_input_pump(
    state: &Rc<RefCell<AppState>>,
    wake_rx: smol::channel::Receiver<()>,
    cx: &mut App,
) {
    let state = state.clone();
    let pump_cx = cx.to_async();
    cx.to_async()
        .foreground_executor()
        .spawn(async move {
            while wake_rx.recv().await.is_ok() {
                let _ = pump_cx.update(|cx| {
                    let actions = state.borrow().token_input_results.drain();
                    for action in actions {
                        crate::bootstrap::dispatch_in_app(&state, action, cx);
                    }
                });
            }
        })
        .detach();
}
