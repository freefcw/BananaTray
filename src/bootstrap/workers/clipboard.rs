use crate::runtime::AppState;
use log::{info, warn};
use std::cell::RefCell;
use std::rc::Rc;

pub(crate) type ClipboardSender = crate::runtime::BackgroundJobSender<String>;
pub(crate) type ClipboardReceiver = crate::runtime::BackgroundJobReceiver<String>;

pub(crate) fn clipboard_channel() -> (ClipboardSender, ClipboardReceiver) {
    crate::runtime::BackgroundJobSender::channel(8)
}

/// 在独立阻塞线程执行剪贴板写入。
///
/// `xclip` / `xsel` 可能为了持有选区而保持进程存活；即使命令本身有超时，也不能在
/// GPUI 前台调用它们，否则一次复制失败仍会短暂冻结托盘和设置窗口。
pub(crate) fn start_clipboard_worker(
    state: &Rc<RefCell<AppState>>,
    clipboard_rx: ClipboardReceiver,
) {
    let owner = crate::utils::BoundedThreadOwner::spawn("clipboard-worker", move || {
        while let Some(text) = clipboard_rx.recv() {
            let byte_len = text.len();
            match crate::platform::system::copy_to_clipboard(&text) {
                Ok(()) => info!(target: "settings", "copied {byte_len} bytes to clipboard"),
                Err(err) => warn!(target: "settings", "failed to copy to clipboard: {err:#}"),
            }
        }
    })
    .expect("failed to spawn clipboard worker");
    state.borrow().clipboard_tx.attach_owner(owner);
}
