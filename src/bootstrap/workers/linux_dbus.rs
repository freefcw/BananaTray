use crate::runtime::AppState;
use log::warn;
use std::cell::RefCell;
use std::rc::Rc;

struct RegisteredDBusHandle(Rc<RefCell<Option<crate::dbus::DBusServiceHandle>>>);

impl gpui::Global for RegisteredDBusHandle {}

pub(crate) fn register_dbus_snapshot_global(
    handle: Rc<RefCell<Option<crate::dbus::DBusServiceHandle>>>,
    cx: &mut gpui::App,
) {
    cx.set_global(RegisteredDBusHandle(handle));
}

pub(crate) fn emit_registered_dbus_snapshot(state: &Rc<RefCell<AppState>>, cx: &mut gpui::App) {
    let Some(handle) = cx
        .try_global::<RegisteredDBusHandle>()
        .map(|global| global.0.clone())
    else {
        return;
    };
    if !emit_current_dbus_snapshot(state, handle.borrow().as_ref()) {
        handle.borrow_mut().take();
    }
}

/// 向 GNOME Shell Extension 发射当前状态快照。
///
/// 返回 `false` 表示发射通道已永久失效（D-Bus 线程退出后 signal channel 关闭，
/// 或快照缓存 poisoned）；调用方应放弃 handle，一次性降级为不再发射，
/// 否则连接失败环境下每次刷新都会重复这条 warn。
pub(crate) fn emit_current_dbus_snapshot(
    state: &Rc<RefCell<AppState>>,
    handle: Option<&crate::dbus::DBusServiceHandle>,
) -> bool {
    use crate::application::DBusQuotaSnapshot;

    if let Some(handle) = handle {
        let state_ref = state.borrow();
        let snapshot = DBusQuotaSnapshot::from_session(&state_ref.session);
        match serde_json::to_string(&snapshot) {
            Ok(json) => {
                if let Err(e) = handle.emit_refresh_complete(json) {
                    warn!(target: "dbus", "failed to emit RefreshComplete: {e}; disabling snapshot emission for this session");
                    return false;
                }
            }
            Err(e) => {
                warn!(target: "dbus", "failed to serialize D-Bus snapshot: {e}");
            }
        }
    }
    true
}
