//! D-Bus 服务模块 — Linux 专用
//!
//! 提供 `com.bananatray.Daemon` D-Bus 接口，供 GNOME Shell Extension 查询配额数据。
//! 受 `cfg(target_os = "linux")` 和 `cfg(feature = "app")` 门控。
//!
//! ## 线程模型
//!
//! ```text
//! 主线程 (GPUI)                          D-Bus 线程
//!   |                                       |
//!   +-- snapshot_cache.update(json) ---->   +-- GetAllQuotas 读取 snapshot_cache
//!   +-- signal_tx.send(()) ------------>   +-- RefreshAll 读取 snapshot_cache + 通知 GPUI
//!   |                                       +-- ObjectServer 处理方法调用
//!   |  <-- action_rx.recv() -------------  |
//!   +-- bootstrap::dispatch_in_app() 处理   +-- iface_ref.refresh_complete(json)
//!      OpenSettings / RefreshAll           └─ smol 异步执行器
//! ```
//!
//! **关键设计**：`BananaTrayIface` 持有 `Arc<Mutex<String>>` 快照缓存，
//! 不持有 `AppState`。这满足了 zbus `Interface: Send + Sync` 的约束。

// `dbus` 同时被 lib target 编译，但实际启动入口在 bin target。
// 对 lib target 来说这些类型会表现为未使用；保留编译覆盖即可。
#![allow(dead_code)]

mod iface;
mod serde_types;

#[allow(unused_imports)]
pub use serde_types::{DBusHeaderInfo, DBusProviderEntry, DBusQuotaEntry, DBusQuotaSnapshot};

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use log::{info, warn};

use crate::runtime::AppState;

use iface::{BananaTrayIface, BananaTrayIfaceSignals, DBusActionRequest};

// ============================================================================
// 服务 Handle（主线程持有）
// ============================================================================

/// D-Bus 服务句柄，允许主线程更新快照缓存并发射信号
pub struct DBusServiceHandle {
    /// 快照 JSON 缓存（与 iface 共享）
    snapshot_cache: Arc<Mutex<String>>,
    /// 信号发射通道
    signal_tx: smol::channel::Sender<()>,
    worker: Arc<Mutex<Option<crate::utils::BoundedThreadOwner>>>,
}

impl Drop for DBusServiceHandle {
    fn drop(&mut self) {
        self.signal_tx.close();
        if let Ok(mut slot) = self.worker.lock() {
            if let Some(worker) = slot.as_mut() {
                let _ = worker.shutdown_before(
                    std::time::Instant::now() + std::time::Duration::from_millis(20),
                );
            }
            *slot = None;
        }
    }
}

impl DBusServiceHandle {
    /// 更新快照缓存并发射 RefreshComplete 信号
    pub fn emit_refresh_complete(&self, json_data: String) -> Result<()> {
        // 1. 更新缓存（供 GetAllQuotas / RefreshAll 读取）
        let mut cache = self
            .snapshot_cache
            .lock()
            .map_err(|e| anyhow::anyhow!("D-Bus snapshot cache lock failed: {e}"))?;
        *cache = json_data;
        drop(cache);

        // 2. 通知 D-Bus 线程读取最新缓存并发射信号。队列已满表示已有唤醒待处理，
        // 最新快照已经写入缓存，因此无需再排一个重复唤醒。
        match self.signal_tx.try_send(()) {
            Ok(()) | Err(smol::channel::TrySendError::Full(())) => Ok(()),
            Err(smol::channel::TrySendError::Closed(())) => {
                Err(anyhow::anyhow!("D-Bus signal channel closed"))
            }
        }
    }
}

// ============================================================================
// 服务启动
// ============================================================================

/// 启动 D-Bus 服务，返回 handle 供主线程更新缓存和发射信号
///
/// D-Bus 服务在独立线程运行，使用自己的 smol 执行器。
/// `action_rx` 的消费在 GPUI 的 foreground executor 上进行。
pub fn start_dbus_service(
    state: Rc<RefCell<AppState>>,
    async_cx: gpui::AsyncApp,
) -> Option<DBusServiceHandle> {
    let snapshot_cache = Arc::new(Mutex::new(String::from("{}")));
    let (signal_tx, signal_rx) = smol::channel::bounded::<()>(1);
    let (action_tx, action_rx) = smol::channel::bounded::<DBusActionRequest>(8);

    let handle = DBusServiceHandle {
        snapshot_cache: snapshot_cache.clone(),
        signal_tx,
        worker: Arc::new(Mutex::new(None)),
    };

    // 桥接 D-Bus 动作请求到 GPUI 主线程（复用 foreground executor，无需额外线程）
    spawn_action_bridge(state, action_rx, async_cx);

    // 启动 D-Bus 服务线程
    let spawn_result = crate::utils::BoundedThreadOwner::spawn("dbus-service", move || {
        run_dbus_server(snapshot_cache, action_tx, signal_rx);
    });

    match spawn_result {
        Ok(owner) => {
            if let Ok(mut slot) = handle.worker.lock() {
                *slot = Some(owner);
            }
            info!(target: "dbus", "D-Bus service thread started");
            Some(handle)
        }
        Err(e) => {
            warn!(target: "dbus", "failed to spawn D-Bus service thread: {e}");
            None
        }
    }
}

/// 桥接 D-Bus 动作请求到 GPUI 主线程
///
/// 复用 GPUI foreground executor 消费 action_rx，避免额外线程开销。
/// `state` 仅在此处使用（GPUI 主线程），不会 move 到 D-Bus 线程。
fn spawn_action_bridge(
    state: Rc<RefCell<AppState>>,
    action_rx: smol::channel::Receiver<DBusActionRequest>,
    async_cx: gpui::AsyncApp,
) {
    let ui_cx = async_cx.clone();
    async_cx
        .foreground_executor()
        .spawn(async move {
            while let Ok(action) = action_rx.recv().await {
                match action {
                    DBusActionRequest::OpenSettings => {
                        info!(target: "dbus", "scheduling OpenSettings on GPUI main thread");
                        let _ = ui_cx.update(|cx| {
                            crate::bootstrap::dispatch_in_app(
                                &state,
                                crate::application::AppAction::OpenSettings { provider: None },
                                cx,
                            );
                        });
                    }
                    DBusActionRequest::RefreshAll => {
                        info!(target: "dbus", "scheduling RefreshAll on GPUI main thread");
                        let _ = ui_cx.update(|cx| {
                            crate::bootstrap::dispatch_in_app(
                                &state,
                                crate::application::AppAction::RefreshAll,
                                cx,
                            );
                        });
                    }
                }
            }
        })
        .detach();
}

/// D-Bus 服务主循环（在独立线程运行）
fn run_dbus_server(
    snapshot_cache: Arc<Mutex<String>>,
    action_tx: smol::channel::Sender<DBusActionRequest>,
    signal_rx: smol::channel::Receiver<()>,
) {
    if let Err(e) = smol::block_on(async {
        let iface = BananaTrayIface::new(snapshot_cache.clone(), action_tx);

        // 连接 Session Bus
        let conn = zbus::connection::Builder::session()?
            .name("com.bananatray.Daemon")?
            .serve_at("/com/bananatray/Daemon", iface)?
            .build()
            .await?;

        info!(target: "dbus", "D-Bus service registered on session bus");

        // 获取 interface 引用用于发射信号
        let iface_ref = conn
            .object_server()
            .interface::<_, BananaTrayIface>("/com/bananatray/Daemon")
            .await?;

        // 等待信号请求并发射
        while signal_rx.recv().await.is_ok() {
            let json_data = match snapshot_cache.lock() {
                Ok(cache) => cache.clone(),
                Err(err) => {
                    warn!(target: "dbus", "failed to read snapshot for RefreshComplete: {err}");
                    continue;
                }
            };
            if let Err(e) = iface_ref.refresh_complete(&json_data).await {
                warn!(target: "dbus", "failed to emit RefreshComplete signal: {e}");
            }
        }

        Ok::<(), zbus::Error>(())
    }) {
        warn!(target: "dbus", "D-Bus service error: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn handle_with_capacity(capacity: usize) -> (DBusServiceHandle, smol::channel::Receiver<()>) {
        let snapshot_cache = Arc::new(Mutex::new(String::from("{}")));
        let (signal_tx, signal_rx) = smol::channel::bounded(capacity);
        (
            DBusServiceHandle {
                snapshot_cache,
                signal_tx,
                worker: Arc::new(Mutex::new(None)),
            },
            signal_rx,
        )
    }

    #[test]
    fn refresh_signal_coalesces_when_wakeup_is_already_queued() {
        let (handle, signal_rx) = handle_with_capacity(1);

        handle.emit_refresh_complete("first".into()).unwrap();
        handle.emit_refresh_complete("latest".into()).unwrap();

        assert_eq!(*handle.snapshot_cache.lock().unwrap(), "latest");
        assert_eq!(signal_rx.len(), 1);
    }

    #[test]
    fn refresh_signal_reports_closed_channel() {
        let (handle, signal_rx) = handle_with_capacity(1);
        drop(signal_rx);

        let err = handle.emit_refresh_complete("latest".into()).unwrap_err();

        assert!(err.to_string().contains("channel closed"));
    }

    #[test]
    fn poisoned_cache_does_not_queue_refresh_signal() {
        let (handle, signal_rx) = handle_with_capacity(1);
        let cache = handle.snapshot_cache.clone();
        let _ = std::thread::spawn(move || {
            let _guard = cache.lock().unwrap();
            panic!("poison snapshot cache");
        })
        .join();

        let err = handle.emit_refresh_complete("latest".into()).unwrap_err();

        assert!(err.to_string().contains("cache lock failed"));
        assert!(signal_rx.is_empty());
    }
}
