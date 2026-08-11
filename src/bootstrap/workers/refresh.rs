use crate::application::AppAction;
use crate::refresh::{RefreshCoordinator, RefreshReason, RefreshRequest};
use crate::runtime::AppState;
use gpui::App;
use log::warn;
use std::cell::RefCell;
use std::rc::Rc;

use crate::bootstrap::capabilities::dispatch_in_app;

/// 创建 ProviderManager + RefreshCoordinator，启动后台刷新线程。
/// 返回 (refresh_tx, event_rx, manager) 供后续步骤使用。
pub(crate) fn bootstrap_refresh() -> (
    smol::channel::Sender<RefreshRequest>,
    smol::channel::Receiver<crate::refresh::RefreshEvent>,
    crate::providers::ProviderManagerHandle,
) {
    let (event_tx, event_rx) = smol::channel::bounded::<crate::refresh::RefreshEvent>(64);
    let manager = crate::providers::ProviderManagerHandle::default();
    let coordinator = RefreshCoordinator::new(manager.clone(), event_tx);
    let refresh_tx = coordinator.sender();

    std::thread::Builder::new()
        .name("refresh-coordinator".into())
        .spawn(move || smol::block_on(coordinator.run()))
        .expect("failed to spawn refresh coordinator thread");

    (refresh_tx, event_rx, manager)
}

/// 启动事件泵：从协调器接收 RefreshEvent，分派到 UI 线程更新 AppState。
#[cfg(target_os = "linux")]
pub(crate) fn start_event_pump(
    state: &Rc<RefCell<AppState>>,
    event_rx: smol::channel::Receiver<crate::refresh::RefreshEvent>,
    dbus_handle: Option<crate::dbus::DBusServiceHandle>,
    cx: &mut App,
) {
    let state = state.clone();
    let pump_cx = cx.to_async();
    cx.to_async()
        .foreground_executor()
        .spawn(async move {
            while let Ok(event) = event_rx.recv().await {
                let _ = pump_cx.update(|cx| {
                    dispatch_in_app(&state, AppAction::RefreshEventReceived(event), cx);
                    super::linux_dbus::emit_current_dbus_snapshot(&state, dbus_handle.as_ref());
                });
            }
        })
        .detach();
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn start_event_pump(
    state: &Rc<RefCell<AppState>>,
    event_rx: smol::channel::Receiver<crate::refresh::RefreshEvent>,
    cx: &mut App,
) {
    let state = state.clone();
    let pump_cx = cx.to_async();
    cx.to_async()
        .foreground_executor()
        .spawn(async move {
            while let Ok(event) = event_rx.recv().await {
                let _ = pump_cx.update(|cx| {
                    dispatch_in_app(&state, AppAction::RefreshEventReceived(event), cx);
                });
            }
        })
        .detach();
}

/// 发送初始配置同步 + 启动首次刷新。
pub(crate) fn trigger_initial_refresh(state: &Rc<RefCell<AppState>>) {
    let (config_request, ids) = {
        let state_ref = state.borrow();
        let session = &state_ref.session;
        (
            crate::application::build_config_sync_request(session),
            session
                .provider_store
                .refreshable_provider_ids(&session.settings),
        )
    };
    if let Err(e) = state.borrow().send_refresh(config_request) {
        warn!(target: "app", "failed to send initial config sync: {e}");
    }
    if let Err(e) = state.borrow().send_refresh(RefreshRequest::RefreshAll {
        ids,
        reason: RefreshReason::Startup,
    }) {
        warn!(target: "app", "failed to send initial refresh: {e}");
    }
}
