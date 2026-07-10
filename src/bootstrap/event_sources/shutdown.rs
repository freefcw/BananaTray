use crate::runtime::AppState;
use gpui::App;
use std::cell::RefCell;
use std::rc::Rc;

/// 注册应用退出钩子，确保后台设置写入线程完成最终落盘后再结束进程。
pub(crate) fn register_app_shutdown(state: &Rc<RefCell<AppState>>, cx: &mut App) {
    let state = state.clone();
    cx.on_app_quit(move |_| {
        // GPUI 只给异步退出 observer 100ms；这里先同步 join，再返回已完成 future。
        state.borrow_mut().shutdown_settings_writer();
        std::future::ready(())
    })
    .detach();
}
