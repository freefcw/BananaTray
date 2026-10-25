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
    crate::bootstrap::spawn_foreground_pump(cx.to_async(), wake_rx, move |(), cx| {
        let actions = state.borrow().token_input_results.drain();
        for action in actions {
            crate::bootstrap::dispatch_in_app(&state, action, cx);
        }
    });
}
