use crate::tray::TrayController;
use gpui::App;
use log::info;
use std::cell::RefCell;
use std::rc::Rc;

/// 监听二次实例的 SHOW 请求，桥接 std::sync::mpsc → 前台 executor。
pub(crate) fn listen_for_secondary_instance(
    controller: &Rc<RefCell<TrayController>>,
    show_rx: std::sync::mpsc::Receiver<()>,
    cx: &mut App,
) {
    let (show_async_tx, show_async_rx) = smol::channel::bounded::<()>(4);
    std::thread::Builder::new()
        .name("single-instance-bridge".into())
        .spawn(move || {
            while show_rx.recv().is_ok() {
                if show_async_tx.send_blocking(()).is_err() {
                    break;
                }
            }
        })
        .expect("failed to spawn single-instance bridge thread");

    let ctrl = controller.clone();
    crate::bootstrap::spawn_foreground_pump(cx.to_async(), show_async_rx, move |(), cx| {
        info!(target: "app", "secondary instance requested SHOW");
        ctrl.borrow_mut().toggle_popup(cx);
    });
}
