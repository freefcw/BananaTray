//! 设置文件 Debounce 写入器
//!
//! 所有设置持久化（debounce 和同步）统一通过此 writer 串行化执行，
//! 避免并发/乱序写入风险。
//!
//! - `schedule()` — 异步 debounce 写入，合并短时间内的多次请求
//! - `flush()` — 同步写入，立即落盘并返回结果（会打断未落盘的 debounce 窗口）

use crate::models::AppSettings;
use crate::settings_store;
use log::{debug, warn};
use std::sync::mpsc;
use std::time::Duration;

/// 默认 debounce 窗口
const DEFAULT_DEBOUNCE: Duration = Duration::from_millis(500);

/// 发送给后台线程的命令
enum WriteCmd {
    /// 异步 debounce 写入：合并窗口内的多次调用，只写最后一份
    Schedule(AppSettings),
    /// 同步写入：立即落盘，通过 reply channel 返回成功/失败
    Flush(AppSettings, mpsc::Sender<bool>),
}

/// 设置文件写入器句柄
///
/// 所有设置持久化都通过此句柄提交，后台线程串行化执行，
/// 保证不会出现旧快照覆盖新快照的乱序问题。
pub(crate) struct SettingsWriter {
    tx: mpsc::Sender<WriteCmd>,
}

impl SettingsWriter {
    /// 启动后台写入线程，返回写入器句柄
    pub fn spawn() -> Self {
        Self::spawn_internal(DEFAULT_DEBOUNCE, Box::new(settings_store::persist))
    }

    fn spawn_internal(
        debounce: Duration,
        persist_fn: Box<dyn Fn(&AppSettings) -> bool + Send>,
    ) -> Self {
        let (tx, rx) = mpsc::channel::<WriteCmd>();

        std::thread::Builder::new()
            .name("settings-writer".into())
            .spawn(move || run_loop(rx, debounce, &*persist_fn))
            .expect("failed to spawn settings-writer thread");

        Self { tx }
    }

    /// 提交一份 settings 快照，后台线程会在 debounce 窗口结束后写盘。
    /// 多次快速调用只会写入最后一份。
    pub fn schedule(&self, settings: AppSettings) {
        if let Err(e) = self.tx.send(WriteCmd::Schedule(settings)) {
            warn!(target: "settings", "settings-writer channel closed: {e}");
        }
    }

    /// 同步写入：立即落盘并返回结果。
    /// 后台线程会先丢弃未落盘的 debounce 快照，确保此次写入是最终状态。
    pub fn flush(&self, settings: AppSettings) -> bool {
        let (reply_tx, reply_rx) = mpsc::channel();
        if self.tx.send(WriteCmd::Flush(settings, reply_tx)).is_err() {
            warn!(target: "settings", "settings-writer channel closed, flush failed");
            return false;
        }
        reply_rx.recv().unwrap_or(false)
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

        // 第一次
        writer.schedule(make_settings(10));
        thread::sleep(Duration::from_millis(100)); // 等待 debounce 结束

        // 第二次
        writer.schedule(make_settings(20));
        thread::sleep(Duration::from_millis(100));

        let r = records.lock().unwrap();
        assert_eq!(r.len(), 2, "separate bursts should produce 2 writes");
        assert_eq!(r[0], 10);
        assert_eq!(r[1], 20);
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
    fn channel_close_triggers_final_flush() {
        let (writer, records) = test_writer(5000); // 很长的 debounce

        // schedule 一个值然后立即 drop writer（关闭 channel）
        writer.schedule(make_settings(77));
        drop(writer);

        // 给后台线程时间完成 final flush
        thread::sleep(Duration::from_millis(100));

        let r = records.lock().unwrap();
        assert_eq!(*r, vec![77], "should flush on channel close");
    }
}
