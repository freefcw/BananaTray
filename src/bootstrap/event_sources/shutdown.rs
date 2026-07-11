use crate::runtime::AppState;
use gpui::App;
use std::cell::RefCell;
use std::rc::Rc;

/// 注册应用退出钩子，确保设置和 launch-at-login 最终状态完成同步后再结束进程。
pub(crate) fn register_app_shutdown(state: &Rc<RefCell<AppState>>, cx: &mut App) {
    let state = state.clone();
    cx.on_app_quit(move |_| {
        // GPUI 只给异步退出 observer 100ms；这里先同步 join，再返回已完成 future。
        let start_at_login = {
            let mut state = state.borrow_mut();
            state.shutdown_settings_writer();
            state.session.settings.system.start_at_login
        };
        crate::platform::auto_launch::sync_and_wait(start_at_login);
        std::future::ready(())
    })
    .detach();
}
