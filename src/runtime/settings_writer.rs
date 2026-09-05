//! 设置文件 Debounce 写入器
//!
//! 所有设置持久化（debounce 和同步）统一通过此 writer 串行化执行，
//! 避免并发/乱序写入风险。
//!
//! - `schedule()` — 异步 debounce 写入，合并短时间内的多次请求
//! - `flush()` — 同步写入，立即落盘并返回结果（会打断未落盘的 debounce 窗口）

use crate::models::AppSettings;
use crate::settings_store;
use crate::utils::BoundedThreadOwner;
use log::{debug, warn};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

/// 默认 debounce 窗口
const DEFAULT_DEBOUNCE: Duration = Duration::from_millis(500);

/// 发送给后台线程的命令
enum WriteCmd {
    /// 异步 debounce 写入：合并窗口内的多次调用，只写最后一份
    Schedule(AppSettings),
    /// 同步写入：立即落盘，通过 reply channel 返回成功/失败
    Flush(AppSettings, mpsc::Sender<bool>),
    /// 完成当前 pending snapshot 的最终写入后退出。
    Shutdown,
}

/// 设置文件写入器句柄
///
/// 所有设置持久化都通过此句柄提交，后台线程串行化执行，
/// 保证不会出现旧快照覆盖新快照的乱序问题。
pub(crate) struct SettingsWriter {
    tx: Option<mpsc::Sender<WriteCmd>>,
    snapshots: Arc<Mutex<SnapshotState>>,
    worker: Option<BoundedThreadOwner>,
}

#[derive(Debug, Clone)]
pub(crate) struct DeferredSettingsFlush {
    revision: u64,
    settings: AppSettings,
}

#[derive(Default)]
struct SnapshotState {
    next_revision: u64,
    latest_committed: Option<DeferredSettingsFlush>,
}

#[derive(Clone)]
pub(crate) struct SettingsWriterHandle {
    tx: mpsc::Sender<WriteCmd>,
    snapshots: Arc<Mutex<SnapshotState>>,
}

impl SettingsWriterHandle {
    pub(crate) fn flush_deferred(&self, deferred: DeferredSettingsFlush) -> bool {
        let reply_rx = {
            let mut snapshots = self
                .snapshots
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let settings = match &snapshots.latest_committed {
                Some(latest) if latest.revision > deferred.revision => latest.settings.clone(),
                _ => {
                    snapshots.latest_committed = Some(deferred.clone());
                    deferred.settings
                }
            };
            send_flush(&self.tx, settings)
        };
        await_flush(reply_rx)
    }
}

impl SettingsWriter {
    /// 启动后台写入线程，返回写入器句柄
    pub fn spawn() -> Self {
        Self::spawn_internal(DEFAULT_DEBOUNCE, Box::new(settings_store::persist))
    }

    #[cfg(test)]
    pub(crate) fn spawn_for_test(
        persist_fn: impl Fn(&AppSettings) -> bool + Send + 'static,
    ) -> Self {
        Self::spawn_internal(Duration::from_secs(60), Box::new(persist_fn))
    }

    fn spawn_internal(
        debounce: Duration,
        persist_fn: Box<dyn Fn(&AppSettings) -> bool + Send>,
    ) -> Self {
        let (tx, rx) = mpsc::channel::<WriteCmd>();

        let worker = BoundedThreadOwner::spawn("settings-writer", move || {
            run_loop(rx, debounce, &*persist_fn)
        })
        .expect("failed to spawn settings-writer thread");

        Self {
            tx: Some(tx),
            snapshots: Arc::new(Mutex::new(SnapshotState::default())),
            worker: Some(worker),
        }
    }

    /// 提交一份 settings 快照，后台线程会在 debounce 窗口结束后写盘。
    /// 多次快速调用只会写入最后一份。
    pub fn schedule(&self, settings: AppSettings) {
        let Some(tx) = &self.tx else {
            warn!(target: "settings", "settings-writer already shut down, schedule ignored");
            return;
        };
        let mut snapshots = self
            .snapshots
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let snapshot = snapshots.commit(settings);
        if let Err(e) = tx.send(WriteCmd::Schedule(snapshot.settings)) {
            warn!(target: "settings", "settings-writer channel closed: {e}");
        }
    }

    /// 创建一份尚未承诺持久化的快照，供后台文件任务成功后提交。
    ///
    /// 后台任务完成前若已有更新设置被 `schedule` / `flush` 提交，迟到的
    /// deferred flush 会改为写入较新的已提交快照，避免旧状态覆盖新状态。
    pub(crate) fn defer_flush(&self, settings: AppSettings) -> DeferredSettingsFlush {
        self.snapshots
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .reserve(settings)
    }

    /// 同步写入：立即落盘并返回结果。
    /// 后台线程会先丢弃未落盘的 debounce 快照，确保此次写入是最终状态。
    pub fn flush(&self, settings: AppSettings) -> bool {
        let Some(tx) = &self.tx else {
            warn!(target: "settings", "settings-writer already shut down, flush failed");
            return false;
        };
        let reply_rx = {
            let mut snapshots = self
                .snapshots
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let snapshot = snapshots.commit(settings);
            send_flush(tx, snapshot.settings)
        };
        await_flush(reply_rx)
    }

    pub(crate) fn handle(&self) -> Option<SettingsWriterHandle> {
        self.tx.as_ref().cloned().map(|tx| SettingsWriterHandle {
            tx,
            snapshots: self.snapshots.clone(),
        })
    }

    /// 关闭发送端并等待后台线程完成最终写入。
    pub fn shutdown_and_join(&mut self) {
        self.request_shutdown();
        let Some(worker) = self.worker.as_mut() else {
            return;
        };
        let _ = worker.join();
        self.worker = None;
    }

    pub(crate) fn request_shutdown(&mut self) {
        let Some(tx) = self.tx.take() else {
            return;
        };
        if tx.send(WriteCmd::Shutdown).is_err() {
            warn!(target: "settings", "settings-writer channel closed before shutdown request");
        }
    }

    pub(crate) fn join_before(&mut self, deadline: std::time::Instant) -> bool {
        let Some(worker) = self.worker.as_mut() else {
            return true;
        };
        let stopped = worker.shutdown_before(deadline);
        if stopped {
            self.worker = None;
        }
        stopped
    }

    pub(crate) fn shutdown_before(&mut self, deadline: std::time::Instant) -> bool {
        self.request_shutdown();
        self.join_before(deadline)
    }

    #[cfg(test)]
    pub(crate) fn is_shutdown(&self) -> bool {
        self.tx.is_none() && self.worker.is_none()
    }
}

impl SnapshotState {
    fn reserve(&mut self, settings: AppSettings) -> DeferredSettingsFlush {
        self.next_revision = self
            .next_revision
            .checked_add(1)
            .expect("settings snapshot revision exhausted");
        DeferredSettingsFlush {
            revision: self.next_revision,
            settings,
        }
    }

    fn commit(&mut self, settings: AppSettings) -> DeferredSettingsFlush {
        let snapshot = self.reserve(settings);
        self.latest_committed = Some(snapshot.clone());
        snapshot
    }
}

fn send_flush(tx: &mpsc::Sender<WriteCmd>, settings: AppSettings) -> Option<mpsc::Receiver<bool>> {
    let (reply_tx, reply_rx) = mpsc::channel();
    if tx.send(WriteCmd::Flush(settings, reply_tx)).is_err() {
        warn!(target: "settings", "settings-writer channel closed, flush failed");
        return None;
    }
    Some(reply_rx)
}

fn await_flush(reply_rx: Option<mpsc::Receiver<bool>>) -> bool {
    reply_rx.and_then(|rx| rx.recv().ok()).unwrap_or(false)
}

impl Drop for SettingsWriter {
    fn drop(&mut self) {
        self.shutdown_before(std::time::Instant::now() + Duration::from_millis(80));
    }
}

/// 后台线程主循环
fn run_loop(
    rx: mpsc::Receiver<WriteCmd>,
    debounce: Duration,
    persist_fn: &dyn Fn(&AppSettings) -> bool,
) {
    loop {
        // 阻塞等待第一条命令
        let cmd = match rx.recv() {
            Ok(cmd) => cmd,
            Err(_) => {
                debug!(target: "settings", "settings-writer channel closed, exiting");
                return;
            }
        };

        match cmd {
            WriteCmd::Shutdown => {
                debug!(target: "settings", "settings-writer shutdown requested");
                return;
            }
            WriteCmd::Flush(settings, reply) => {
                let ok = persist_fn(&settings);
                let _ = reply.send(ok);
            }
            WriteCmd::Schedule(mut latest) => {
                // 进入 debounce 窗口，持续消费直到超时或收到 Flush
                loop {
                    match rx.recv_timeout(debounce) {
                        Ok(WriteCmd::Schedule(newer)) => {
                            // 合并：用更新的快照覆盖，重置计时
                            latest = newer;
                        }
                        Ok(WriteCmd::Flush(settings, reply)) => {
                            // Flush 打断 debounce：丢弃挂起的 Schedule 快照，
                            // 直接写入 Flush 携带的（更新的）快照
                            let ok = persist_fn(&settings);
                            let _ = reply.send(ok);
                            break;
                        }
                        Ok(WriteCmd::Shutdown) => {
                            persist_fn(&latest);
                            debug!(target: "settings", "settings-writer: final flush before exit");
                            return;
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            // 窗口期结束，写盘
                            debug!(target: "settings", "settings-writer: debounce elapsed, persisting");
                            persist_fn(&latest);
                            break;
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => {
                            // Channel 关闭，写入最后一份后退出
                            persist_fn(&latest);
                            debug!(target: "settings", "settings-writer: final flush before exit");
                            return;
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Instant;

    fn make_settings(interval: u64) -> AppSettings {
        AppSettings {
            system: crate::models::SystemSettings {
                refresh_interval_mins: interval,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// 创建一个测试用 writer 和与之配对的记录器
    fn test_writer(debounce_ms: u64) -> (SettingsWriter, Arc<Mutex<Vec<u64>>>) {
        let records: Arc<Mutex<Vec<u64>>> = Arc::new(Mutex::new(Vec::new()));
        let records_clone = records.clone();

        let writer = SettingsWriter::spawn_internal(
            Duration::from_millis(debounce_ms),
            Box::new(move |settings| {
                records_clone
                    .lock()
                    .unwrap()
                    .push(settings.system.refresh_interval_mins);
                true
            }),
        );

        (writer, records)
    }

    /// 等到至少 n 次 persist 完成。不能用固定 sleep：CI 上 worker 晚启动时，
    /// 第二次 schedule 会掉进第一次的 debounce 窗口，被合并成一次写入。
    fn wait_writes(records: &Arc<Mutex<Vec<u64>>>, n: usize) {
        let start = Instant::now();
        loop {
            if records.lock().unwrap().len() >= n {
                return;
            }
            assert!(
                start.elapsed() < Duration::from_secs(1),
                "timed out waiting for {n} writes"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn burst_coalesced_to_single_write() {
        let (writer, records) = test_writer(50);

        // 快速连续提交 3 次
        writer.schedule(make_settings(1));
        writer.schedule(make_settings(2));
        writer.schedule(make_settings(3));

        // 等待 debounce 窗口结束 + 余量
        thread::sleep(Duration::from_millis(200));

        let r = records.lock().unwrap();
        assert_eq!(r.len(), 1, "burst should coalesce to 1 write");
        assert_eq!(r[0], 3, "should persist the last snapshot");
    }

    #[test]
    fn separate_bursts_produce_multiple_writes() {
        let (writer, records) = test_writer(30);

        writer.schedule(make_settings(10));
        wait_writes(&records, 1);

        writer.schedule(make_settings(20));
        wait_writes(&records, 2);

        let r = records.lock().unwrap();
        assert_eq!(*r, vec![10, 20]);
    }

    #[test]
    fn flush_returns_result_synchronously() {
        let (writer, records) = test_writer(50);

        let result = writer.flush(make_settings(42));
        assert!(result);

        let r = records.lock().unwrap();
        assert_eq!(*r, vec![42]);
    }

    #[test]
    fn flush_interrupts_debounce_window() {
        let (writer, records) = test_writer(2000); // 长 debounce 窗口

        // schedule 一个值（开始 2s debounce）
        writer.schedule(make_settings(1));
        thread::sleep(Duration::from_millis(10)); // 确保 schedule 先到达

        // 立即 flush — 不应等 2s
        let start = Instant::now();
        let result = writer.flush(make_settings(99));
        let elapsed = start.elapsed();

        assert!(result);
        assert!(
            elapsed < Duration::from_millis(500),
            "flush should not wait for debounce, took {:?}",
            elapsed
        );

        // flush 应该丢弃 schedule 的快照，只写 flush 的
        let r = records.lock().unwrap();
        assert_eq!(*r, vec![99], "flush should supersede pending schedule");
    }

    #[test]
    fn deferred_flush_does_not_overwrite_newer_scheduled_settings() {
        let (writer, records) = test_writer(2000);
        let deferred = writer.defer_flush(make_settings(1));
        let handle = writer.handle().expect("writer handle");

        writer.schedule(make_settings(2));
        assert!(handle.flush_deferred(deferred));

        assert_eq!(
            *records.lock().unwrap(),
            vec![2],
            "a late background completion must persist the newest committed settings"
        );
    }

    #[test]
    fn drop_waits_for_final_flush() {
        let (writer, records) = test_writer(5000); // 很长的 debounce

        // schedule 一个值然后立即 drop writer（关闭 channel）
        writer.schedule(make_settings(77));
        drop(writer);

        let r = records.lock().unwrap();
        assert_eq!(*r, vec![77], "should flush on channel close");
    }

    #[test]
    fn shutdown_waits_for_final_flush() {
        let (mut writer, records) = test_writer(5000); // 很长的 debounce

        writer.schedule(make_settings(88));
        writer.shutdown_and_join();

        let r = records.lock().unwrap();
        assert_eq!(*r, vec![88], "shutdown should wait for final flush");
    }

    #[test]
    fn shutdown_signal_stops_writer_while_flush_handle_is_still_alive() {
        let (mut writer, records) = test_writer(5000);
        let _flush_handle = writer.handle().expect("writer handle");
        writer.schedule(make_settings(91));

        let stopped = writer.shutdown_before(Instant::now() + Duration::from_millis(200));

        assert!(
            stopped,
            "an external flush handle must not keep shutdown open"
        );
        assert_eq!(*records.lock().unwrap(), vec![91]);
    }

    #[test]
    fn shutdown_is_idempotent() {
        let (mut writer, records) = test_writer(5000);

        writer.schedule(make_settings(99));
        writer.shutdown_and_join();
        writer.shutdown_and_join();

        let r = records.lock().unwrap();
        assert_eq!(*r, vec![99]);
    }
}
