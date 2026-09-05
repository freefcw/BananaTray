use crate::application::{AppAction, NewApiEffect, ScriptProviderEffect};
use crate::models::{
    CustomProviderLifecycleFailure, ScriptProviderConfig, ScriptProviderTestResult,
};
use crate::runtime::settings_writer::DeferredSettingsFlush;
use crate::utils::BoundedThreadOwner;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub(crate) struct CustomProviderResults {
    pending: Arc<Mutex<std::collections::VecDeque<AppAction>>>,
}

impl CustomProviderResults {
    pub(crate) fn push(&self, action: AppAction) {
        self.pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push_back(action);
    }

    pub(crate) fn drain(&self) -> Vec<AppAction> {
        self.pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .drain(..)
            .collect()
    }
}

/// NewAPI 与脚本 Provider CRUD 所需的阻塞文件 I/O。
#[derive(Debug)]
pub(crate) enum CustomProviderJob {
    NewApi {
        effect: NewApiEffect,
        settings: DeferredSettingsFlush,
    },
    ScriptProvider {
        effect: ScriptProviderEffect,
        settings: DeferredSettingsFlush,
    },
}

/// 脚本 Run Test 独占的任务；不能进入 custom-provider I/O 串行队列。
#[derive(Debug)]
pub(crate) struct ScriptTestJob {
    pub(crate) request_id: u64,
    pub(crate) config: ScriptProviderConfig,
}

/// 后台队列发送端同时持有取消状态和线程 owner。
///
/// shutdown 不排入 FIFO；它先发布取消状态再关闭 channel，使 worker 完成当前任务后、
/// 下一次领取任务时立即停止，缓冲任务不会被 drain。
pub(crate) struct BackgroundJobSender<J> {
    tx: smol::channel::Sender<J>,
    cancelled: Arc<AtomicBool>,
    owner: Arc<Mutex<Option<BoundedThreadOwner>>>,
}

pub(crate) struct BackgroundJobReceiver<J> {
    rx: smol::channel::Receiver<J>,
    cancelled: Arc<AtomicBool>,
}

/// 关闭后继续 drain 的持久任务队列；退出超时 detach 时不保证未完成事务结算。
pub(crate) struct PersistentJobSender<J> {
    tx: smol::channel::Sender<J>,
    owner: Arc<Mutex<Option<BoundedThreadOwner>>>,
}

pub(crate) struct PersistentJobReceiver<J> {
    rx: smol::channel::Receiver<J>,
}

impl<J> PersistentJobSender<J> {
    pub(crate) fn channel(capacity: usize) -> (Self, PersistentJobReceiver<J>) {
        let (tx, rx) = smol::channel::bounded(capacity);
        (
            Self {
                tx,
                owner: Arc::new(Mutex::new(None)),
            },
            PersistentJobReceiver { rx },
        )
    }

    pub(crate) fn try_send(&self, job: J) -> Result<(), smol::channel::TrySendError<J>> {
        self.tx.try_send(job)
    }

    pub(crate) fn attach_owner(&self, owner: BoundedThreadOwner) {
        if let Ok(mut slot) = self.owner.lock() {
            *slot = Some(owner);
        }
    }

    /// 拒绝新事务；receiver 会继续读取并完成关闭前已经入队的任务。
    pub(crate) fn close(&self) {
        self.tx.close();
    }

    pub(crate) fn join_before(&self, deadline: std::time::Instant) -> bool {
        let Ok(mut slot) = self.owner.lock() else {
            return false;
        };
        slot.as_mut()
            .map(|owner| owner.shutdown_before(deadline))
            .unwrap_or(true)
    }

    #[cfg(test)]
    pub(crate) fn is_shutdown(&self) -> bool {
        self.tx.is_closed()
    }
}

impl<J> PersistentJobReceiver<J> {
    pub(crate) fn recv(&self) -> Option<J> {
        smol::block_on(self.rx.recv()).ok()
    }

    #[cfg(test)]
    pub(crate) fn try_recv(&self) -> Result<J, smol::channel::TryRecvError> {
        self.rx.try_recv()
    }
}

impl<J> BackgroundJobSender<J> {
    pub(crate) fn channel(capacity: usize) -> (Self, BackgroundJobReceiver<J>) {
        let (tx, rx) = smol::channel::bounded(capacity);
        let cancelled = Arc::new(AtomicBool::new(false));
        (
            Self {
                tx,
                cancelled: cancelled.clone(),
                owner: Arc::new(Mutex::new(None)),
            },
            BackgroundJobReceiver { rx, cancelled },
        )
    }

    pub(crate) fn try_send(&self, job: J) -> Result<(), smol::channel::TrySendError<J>> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err(smol::channel::TrySendError::Closed(job));
        }
        self.tx.try_send(job)
    }

    pub(crate) fn attach_owner(&self, owner: BoundedThreadOwner) {
        if let Ok(mut slot) = self.owner.lock() {
            *slot = Some(owner);
        }
    }

    pub(crate) fn request_shutdown(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.tx.close();
    }

    pub(crate) fn join_before(&self, deadline: std::time::Instant) -> bool {
        let Ok(mut slot) = self.owner.lock() else {
            return false;
        };
        slot.as_mut()
            .map(|owner| owner.shutdown_before(deadline))
            .unwrap_or(true)
    }

    #[cfg(test)]
    pub(crate) fn is_shutdown(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

impl<J> BackgroundJobReceiver<J> {
    /// 阻塞领取一项工作；取消一旦发布，已在 channel 中缓冲的任务也会被丢弃。
    pub(crate) fn recv(&self) -> Option<J> {
        if self.cancelled.load(Ordering::Acquire) {
            return None;
        }
        let job = smol::block_on(self.rx.recv()).ok()?;
        if self.cancelled.load(Ordering::Acquire) {
            None
        } else {
            Some(job)
        }
    }
}

impl CustomProviderJob {
    pub(crate) fn queue_failure(self, detail: String) -> AppAction {
        let failure =
            |operation| CustomProviderLifecycleFailure::file_operation(operation, detail.clone());
        match self {
            Self::NewApi {
                effect:
                    NewApiEffect::SaveProvider {
                        request_id,
                        config,
                        original_filename,
                        original_id,
                        is_editing,
                    },
                ..
            } => AppAction::NewApiSaveFinished {
                request_id,
                filename: original_filename
                    .unwrap_or_else(|| crate::providers::custom::api::generate_filename(&config)),
                config,
                original_id,
                is_editing,
                result: Err(failure("queue NewAPI provider save")),
            },
            Self::NewApi {
                effect:
                    NewApiEffect::DeleteProvider {
                        request_id,
                        provider_id,
                    },
                ..
            } => AppAction::NewApiDeleteFinished {
                request_id,
                provider_id,
                result: Err(failure("queue NewAPI provider delete")),
            },
            Self::NewApi {
                effect: NewApiEffect::LoadConfig { provider_id },
                ..
            } => AppAction::NewApiLoadFinished {
                provider_id,
                result: Err(failure("queue NewAPI provider load")),
            },
            Self::ScriptProvider {
                effect:
                    ScriptProviderEffect::SaveProvider {
                        request_id,
                        config,
                        original_yaml_filename,
                        original_script_filename,
                        is_editing,
                    },
                ..
            } => {
                let yaml_filename = original_yaml_filename.unwrap_or_else(|| {
                    crate::providers::custom::api::generate_script_yaml_filename(&config)
                });
                let script_filename = original_script_filename.unwrap_or_else(|| {
                    crate::providers::custom::api::generate_script_filename(&config)
                });
                AppAction::ScriptProviderSaveFinished {
                    request_id,
                    config,
                    yaml_filename,
                    script_filename,
                    is_editing,
                    result: Err(failure("queue script provider save")),
                }
            }
            Self::ScriptProvider {
                effect:
                    ScriptProviderEffect::DeleteProvider {
                        request_id,
                        provider_id,
                    },
                ..
            } => AppAction::ScriptProviderDeleteFinished {
                request_id,
                provider_id,
                result: Err(failure("queue script provider delete")),
            },
            Self::ScriptProvider {
                effect: ScriptProviderEffect::LoadConfig { provider_id },
                ..
            } => AppAction::ScriptProviderLoadFinished {
                provider_id,
                result: Err(failure("queue script provider load")),
            },
            Self::ScriptProvider {
                effect: ScriptProviderEffect::TestProvider { request_id, config },
                ..
            } => ScriptTestJob { request_id, config }.queue_failure(detail),
        }
    }
}

impl ScriptTestJob {
    pub(crate) fn queue_failure(self, detail: String) -> AppAction {
        AppAction::ScriptProviderTestFinished {
            request_id: self.request_id,
            result: ScriptProviderTestResult {
                success: false,
                message: format!("failed to queue script test: {detail}"),
                stdout: String::new(),
                stderr: String::new(),
                preview: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn shutdown_prevents_buffered_jobs_from_being_executed() {
        let (sender, receiver) = BackgroundJobSender::channel(4);
        sender.try_send(1).expect("queue first job");
        sender.try_send(2).expect("queue second job");

        sender.request_shutdown();

        let mut executed = Vec::new();
        while let Some(job) = receiver.recv() {
            executed.push(job);
        }
        assert!(executed.is_empty(), "shutdown must preempt buffered work");
    }

    #[test]
    fn persistent_shutdown_drains_buffered_jobs_before_joining() {
        let (sender, receiver) = PersistentJobSender::channel(4);
        sender.try_send(1).expect("queue first transaction");
        sender.try_send(2).expect("queue second transaction");

        sender.close();

        let mut executed = Vec::new();
        while let Some(job) = receiver.recv() {
            executed.push(job);
        }
        assert_eq!(executed, vec![1, 2], "persistent work must be drained");
    }

    #[test]
    fn custom_provider_io_is_not_blocked_by_a_running_script_test() {
        let (script_sender, script_receiver) = BackgroundJobSender::channel(1);
        let (io_sender, io_receiver) = BackgroundJobSender::channel(1);
        let (script_started_tx, script_started_rx) = mpsc::sync_channel(0);
        let (release_script_tx, release_script_rx) = mpsc::sync_channel(0);
        let (io_finished_tx, io_finished_rx) = mpsc::channel();

        script_sender.try_send(()).expect("queue script test");
        io_sender.try_send(()).expect("queue custom-provider I/O");

        let script_worker = std::thread::spawn(move || {
            script_receiver.recv().expect("script test job");
            script_started_tx.send(()).expect("report script start");
            release_script_rx.recv().expect("release script test");
        });
        let io_worker = std::thread::spawn(move || {
            io_receiver.recv().expect("custom-provider I/O job");
            io_finished_tx.send(()).expect("report I/O completion");
        });

        script_started_rx
            .recv_timeout(Duration::from_millis(100))
            .expect("script test should start");
        io_finished_rx
            .recv_timeout(Duration::from_millis(100))
            .expect("custom-provider I/O should finish while script test is blocked");
        release_script_tx.send(()).expect("release script test");

        script_worker.join().expect("script worker should stop");
        io_worker.join().expect("I/O worker should stop");
    }
}
