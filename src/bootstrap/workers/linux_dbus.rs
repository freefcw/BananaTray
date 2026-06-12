use crate::runtime::AppState;
use log::warn;
use std::cell::RefCell;
use std::rc::Rc;

/// 向 GNOME Shell Extension 发射当前状态快照。
pub(crate) fn emit_current_dbus_snapshot(
    state: &Rc<RefCell<AppState>>,
    handle: Option<&crate::dbus::DBusServiceHandle>,
) {
    use crate::application::DBusQuotaSnapshot;

    if let Some(handle) = handle {
        let state_ref = state.borrow();
        let snapshot = DBusQuotaSnapshot::from_session(&state_ref.session);
        match serde_json::to_string(&snapshot) {
            Ok(json) => {
                if let Err(e) = handle.emit_refresh_complete(json) {
                    warn!(target: "dbus", "failed to emit RefreshComplete: {e}");
                }
            }
            Err(e) => {
                warn!(target: "dbus", "failed to serialize D-Bus snapshot: {e}");
            }
        }
    }
}
