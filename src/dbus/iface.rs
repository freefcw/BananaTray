//! zbus Interface 实现 — `com.bananatray.Daemon`
//!
//! D-Bus 方法/信号/属性定义。所有方法在 zbus 的异步执行器上运行。
//!
//! **设计要点**：Iface 对象不持有 GPUI 主线程状态（`AppState`），
//! 而是持有 `Arc<Mutex<String>>` 缓存的快照 JSON 和 `action_tx` 通道。
//! 这使得 `BananaTrayIface` 满足 `Send + Sync`（zbus `Interface` trait 要求），
//! 同时避免将 `Rc<RefCell<AppState>>` move 到 D-Bus 线程。

// `dbus` 模块在 lib target 中只参与编译检查，实际由 bin 启动路径注册。
#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use zbus::object_server::SignalEmitter;

/// D-Bus 方法调用产生的动作请求（从 D-Bus 线程发往 GPUI 主线程）
pub(crate) enum DBusActionRequest {
    /// 打开设置窗口
    OpenSettings,
    /// 刷新所有 Provider
    RefreshAll,
}

/// BananaTray D-Bus 接口实现
///
/// 持有 JSON 快照缓存（由 GPUI 主线程更新）和动作请求通道。
/// 满足 `Send + Sync`，可安全地在 zbus ObjectServer 中注册。
pub struct BananaTrayIface {
    /// 缓存的配额快照 JSON（GPUI 主线程写入，D-Bus 线程读取）
    snapshot_cache: Arc<Mutex<String>>,
    /// 动作请求通道（D-Bus → GPUI 主线程）
    action_tx: smol::channel::Sender<DBusActionRequest>,
}

impl BananaTrayIface {
    pub fn new(
        snapshot_cache: Arc<Mutex<String>>,
        action_tx: smol::channel::Sender<DBusActionRequest>,
    ) -> Self {
        Self {
            snapshot_cache,
            action_tx,
        }
    }

    fn schedule_action(
        &self,
        action: DBusActionRequest,
        action_name: &str,
    ) -> zbus::fdo::Result<()> {
        match self.action_tx.try_send(action) {
            Ok(()) => Ok(()),
            Err(smol::channel::TrySendError::Full(_)) => Err(zbus::fdo::Error::LimitsExceeded(
                format!("{action_name} queue is full"),
            )),
            Err(smol::channel::TrySendError::Closed(_)) => Err(zbus::fdo::Error::Failed(format!(
                "{action_name} bridge is closed"
            ))),
        }
    }
}

#[zbus::interface(name = "com.bananatray.Daemon")]
impl BananaTrayIface {
    /// 获取所有已启用 Provider 的配额快照（JSON）
    fn get_all_quotas(&self) -> zbus::fdo::Result<String> {
        self.snapshot_cache
            .lock()
            .map(|guard| guard.clone())
            .map_err(|e| zbus::fdo::Error::Failed(format!("cache lock error: {e}")))
    }

    /// 触发刷新所有 Provider，并返回当前快照（JSON）
    fn refresh_all(&self) -> zbus::fdo::Result<String> {
        // 通知 GPUI 主线程发起刷新（实际刷新是异步的）
        self.schedule_action(DBusActionRequest::RefreshAll, "RefreshAll")?;
        // 返回当前快照（刷新结果将通过 RefreshComplete 信号推送）
        self.snapshot_cache
            .lock()
            .map(|guard| guard.clone())
            .map_err(|e| zbus::fdo::Error::Failed(format!("cache lock error: {e}")))
    }

    /// 打开设置窗口（异步，在 GPUI 主线程执行）
    fn open_settings(&self) -> zbus::fdo::Result<()> {
        self.schedule_action(DBusActionRequest::OpenSettings, "OpenSettings")
    }

    /// 刷新完成信号
    #[zbus(signal)]
    async fn refresh_complete(emitter: &SignalEmitter<'_>, json_data: &str) -> zbus::Result<()>;

    /// Daemon 是否活跃
    #[zbus(property)]
    fn is_active(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn iface_with_action_capacity(
        capacity: usize,
    ) -> (BananaTrayIface, smol::channel::Receiver<DBusActionRequest>) {
        iface_with_snapshot_and_action_capacity("{}", capacity)
    }

    fn iface_with_snapshot_and_action_capacity(
        snapshot: &str,
        capacity: usize,
    ) -> (BananaTrayIface, smol::channel::Receiver<DBusActionRequest>) {
        let (action_tx, action_rx) = smol::channel::bounded(capacity);
        (
            BananaTrayIface::new(Arc::new(Mutex::new(snapshot.into())), action_tx),
            action_rx,
        )
    }

    #[test]
    fn get_all_quotas_returns_cached_snapshot_json() {
        let snapshot = r#"{"schema_version":1,"providers":[]}"#;
        let (iface, _action_rx) = iface_with_snapshot_and_action_capacity(snapshot, 1);

        let result = iface.get_all_quotas().unwrap();

        assert_eq!(result, snapshot);
    }

    #[test]
    fn refresh_all_returns_current_snapshot_and_requests_refresh_all() {
        let snapshot_cache = Arc::new(Mutex::new("initial snapshot".into()));
        let (action_tx, action_rx) = smol::channel::bounded(1);
        let iface = BananaTrayIface::new(snapshot_cache.clone(), action_tx);
        let snapshot = r#"{"schema_version":1,"providers":[{"id":"claude"}]}"#;
        *snapshot_cache.lock().unwrap() = snapshot.into();

        let result = iface.refresh_all().unwrap();

        assert_eq!(result, snapshot);
        assert!(matches!(
            action_rx.try_recv().unwrap(),
            DBusActionRequest::RefreshAll
        ));
    }

    #[test]
    fn open_settings_requests_open_settings() {
        let (iface, action_rx) = iface_with_action_capacity(1);

        iface.open_settings().unwrap();

        assert!(matches!(
            action_rx.try_recv().unwrap(),
            DBusActionRequest::OpenSettings
        ));
    }

    #[test]
    fn full_action_queue_maps_to_limits_exceeded() {
        let (iface, _action_rx) = iface_with_action_capacity(1);
        iface
            .schedule_action(DBusActionRequest::RefreshAll, "first")
            .unwrap();

        let err = iface
            .schedule_action(DBusActionRequest::OpenSettings, "second")
            .unwrap_err();

        assert!(matches!(err, zbus::fdo::Error::LimitsExceeded(_)));
    }

    #[test]
    fn closed_action_queue_maps_to_failed() {
        let (iface, action_rx) = iface_with_action_capacity(1);
        drop(action_rx);

        let err = iface
            .schedule_action(DBusActionRequest::RefreshAll, "RefreshAll")
            .unwrap_err();

        assert!(matches!(err, zbus::fdo::Error::Failed(_)));
    }
}
