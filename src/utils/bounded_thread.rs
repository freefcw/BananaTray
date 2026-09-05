/// 对可能阻塞的后台线程提供有截止时间的所有权回收。
///
/// Rust 无法安全强杀任意线程；到期时丢弃 `JoinHandle`，让应用退出不被慢 I/O
/// 拖住。调用方应先发送 cooperative shutdown 请求，再调用此方法。
pub(crate) struct BoundedThreadOwner {
    name: String,
    worker: Option<std::thread::JoinHandle<()>>,
    done_rx: std::sync::mpsc::Receiver<()>,
}

impl BoundedThreadOwner {
    pub(crate) fn spawn(name: &str, task: impl FnOnce() + Send + 'static) -> std::io::Result<Self> {
        let (done_tx, done_rx) = std::sync::mpsc::sync_channel(1);
        let worker = std::thread::Builder::new()
            .name(name.to_string())
            .spawn(move || {
                struct Done(std::sync::mpsc::SyncSender<()>);
                impl Drop for Done {
                    fn drop(&mut self) {
                        let _ = self.0.try_send(());
                    }
                }
                let _done = Done(done_tx);
                task();
            })?;
        Ok(Self {
            name: name.to_string(),
            worker: Some(worker),
            done_rx,
        })
    }

    /// 在共同截止时间前回收线程；返回 `false` 表示线程已安全 detach。
    pub(crate) fn shutdown_before(&mut self, deadline: std::time::Instant) -> bool {
        let Some(worker) = self.worker.take() else {
            return true;
        };
        let completed = if worker.is_finished() {
            true
        } else if let Some(remaining) = deadline.checked_duration_since(std::time::Instant::now()) {
            matches!(
                self.done_rx.recv_timeout(remaining),
                Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected)
            )
        } else {
            false
        };

        if !completed {
            log::warn!(
                target: "app",
                "{} did not stop before quit deadline; detaching thread",
                self.name
            );
            drop(worker);
            return false;
        }

        if worker.join().is_err() {
            log::warn!(target: "app", "{} panicked during shutdown", self.name);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shutdown_never_waits_past_its_deadline() {
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let mut owner = BoundedThreadOwner::spawn("stuck-test-worker", move || {
            let _ = release_rx.recv();
        })
        .unwrap();
        let start = std::time::Instant::now();

        let stopped = owner.shutdown_before(start + std::time::Duration::from_millis(20));

        assert!(!stopped);
        assert!(
            start.elapsed() < std::time::Duration::from_millis(100),
            "shutdown must preserve GPUI's 100ms quit budget"
        );
        let _ = release_tx.send(());
    }
}
