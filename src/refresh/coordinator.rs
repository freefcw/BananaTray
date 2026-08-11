//! 刷新协调器 — 控制事件循环 + Provider 单飞执行。
//!
//! `RefreshCoordinator` 负责：
//! 1. 持续处理配置、刷新、reload 与 shutdown 请求
//! 2. 在后台线程池并发执行不同 Provider，同时保证同一 Provider single-flight
//! 3. 将 UI timeout 与底层任务完成分开，避免超时后启动重叠任务
//! 4. 配置或 registry 变化后丢弃旧 generation 的结果
//! 5. 将 ProviderError 转换为稳定的 RefreshOutcome
//!
//! 所有 cooldown、eligibility 与周期 deadline 决策委托给 `RefreshScheduler`。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use smol::channel::{Receiver, Sender};

use crate::models::{ProviderId, ProviderSettings};
use crate::providers::{ProviderError, ProviderManager, ProviderManagerHandle, ProviderResult};

use super::scheduler::RefreshScheduler;
use super::types::*;

#[derive(Debug)]
enum TaskMessage {
    Completed {
        id: ProviderId,
        task_id: u64,
        outcome: RefreshOutcome,
    },
    TimedOut {
        id: ProviderId,
        task_id: u64,
        reason: RefreshReason,
    },
}

#[derive(Debug)]
struct ActiveRefresh {
    task_id: u64,
    generation: u64,
    /// timeout 或配置失效已经向前台发送了终态；底层完成时只释放 single-flight。
    result_reported: bool,
}

enum LoopEvent {
    Request(Result<RefreshRequest, smol::channel::RecvError>),
    Task(Result<TaskMessage, smol::channel::RecvError>),
    Periodic,
}

pub struct RefreshCoordinator {
    manager: ProviderManagerHandle,
    request_tx: Sender<RefreshRequest>,
    request_rx: Receiver<RefreshRequest>,
    event_tx: Sender<RefreshEvent>,
    task_tx: Sender<TaskMessage>,
    task_rx: Receiver<TaskMessage>,
    scheduler: RefreshScheduler,
    provider_credentials: ProviderSettings,
    active_refreshes: HashMap<ProviderId, ActiveRefresh>,
    next_task_id: u64,
    config_generation: u64,
}

impl RefreshCoordinator {
    fn provider_refresh_timeout() -> Duration {
        if cfg!(test) {
            Duration::from_millis(100)
        } else {
            Duration::from_secs(30)
        }
    }

    pub fn new(manager: ProviderManagerHandle, event_tx: Sender<RefreshEvent>) -> Self {
        // 请求通道不设容量上限：请求体小、产生速率受 UI 交互自然约束。
        let (request_tx, request_rx) = smol::channel::unbounded();
        let (task_tx, task_rx) = smol::channel::unbounded();
        Self {
            manager,
            request_tx,
            request_rx,
            event_tx,
            task_tx,
            task_rx,
            scheduler: RefreshScheduler::new(),
            provider_credentials: ProviderSettings::default(),
            active_refreshes: HashMap::new(),
            next_task_id: 0,
            config_generation: 0,
        }
    }

    /// Get a sender to send requests to this coordinator.
    pub fn sender(&self) -> Sender<RefreshRequest> {
        self.request_tx.clone()
    }

    // ========================================================================
    // 结果转换
    // ========================================================================

    /// Convert a structured provider refresh result into a `RefreshOutcome` (pure, no side-effects).
    fn build_outcome(
        id: ProviderId,
        result: ProviderResult<crate::models::RefreshData>,
    ) -> RefreshOutcome {
        match result {
            Ok(data) => {
                for q in &data.quotas {
                    log::info!(
                        target: "refresh",
                        "{}: {:?} — used={:.2} / limit={:.2}, detail={:?}, status={:?}",
                        id, q.label_spec, q.used, q.limit, q.detail_spec, q.status_level(),
                    );
                }
                log::debug!(
                    target: "refresh",
                    "{}: account metadata present={}, tier={:?}",
                    id,
                    data.account_email.is_some(),
                    data.account_tier,
                );
                RefreshOutcome {
                    id,
                    result: RefreshResult::Success { data },
                }
            }
            Err(error) => match &error {
                ProviderError::Unavailable { .. } => {
                    log::info!(target: "refresh", "provider {} unavailable: {}", id, error);
                    RefreshOutcome {
                        id,
                        result: RefreshResult::Unavailable {
                            failure: error.to_failure(),
                        },
                    }
                }
                _ => {
                    log::warn!(target: "refresh", "provider {} failed: {}", id, error);
                    let error_kind = error.error_kind();
                    RefreshOutcome {
                        id,
                        result: RefreshResult::Failed {
                            failure: error.to_failure(),
                            error_kind,
                        },
                    }
                }
            },
        }
    }

    // ========================================================================
    // 事件发送
    // ========================================================================

    async fn emit_finished(&self, id: ProviderId, result: RefreshResult) {
        let _ = self
            .event_tx
            .send(RefreshEvent::Finished(RefreshOutcome { id, result }))
            .await;
    }

    async fn send_skip(&self, id: ProviderId, result: RefreshResult) {
        self.emit_finished(id, result).await;
    }

    async fn begin_refresh(&mut self, id: &ProviderId, task_id: u64) {
        self.scheduler.mark_in_flight(id);
        self.active_refreshes.insert(
            id.clone(),
            ActiveRefresh {
                task_id,
                generation: self.config_generation,
                result_reported: false,
            },
        );
        let _ = self
            .event_tx
            .send(RefreshEvent::Started { id: id.clone() })
            .await;
    }

    // ========================================================================
    // 刷新执行
    // ========================================================================

    /// Run a single provider refresh on the blocking thread pool, catching panics.
    async fn run_refresh(
        mgr: Arc<ProviderManager>,
        id: ProviderId,
        provider_credentials: ProviderSettings,
    ) -> RefreshOutcome {
        smol::unblock(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                smol::block_on(mgr.refresh_by_id(&id, &provider_credentials))
            }))
            .unwrap_or_else(|payload| {
                let message = if let Some(s) = payload.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = payload.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "unknown panic payload".to_string()
                };
                log::error!(
                    target: "refresh",
                    "provider {} panicked during refresh: {}",
                    id,
                    message
                );
                Err(ProviderError::fetch_failed("provider panicked"))
            });
            RefreshCoordinator::build_outcome(id, result)
        })
        .await
    }

    fn allocate_task_id(&mut self) -> u64 {
        self.next_task_id = self.next_task_id.wrapping_add(1);
        self.next_task_id
    }

    /// 启动刷新但不等待结果；主循环继续处理配置、reload 和 shutdown。
    async fn start_refresh(&mut self, id: ProviderId, reason: RefreshReason) {
        if let Some(skip) = self.scheduler.check_eligibility(&id, reason) {
            self.send_skip(id, skip).await;
            return;
        }

        let task_id = self.allocate_task_id();
        self.begin_refresh(&id, task_id).await;

        let completion_tx = self.task_tx.clone();
        let completion_id = id.clone();
        let manager = self.manager.snapshot();
        let provider_credentials = self.provider_credentials.clone();
        smol::spawn(async move {
            let outcome =
                Self::run_refresh(manager, completion_id.clone(), provider_credentials).await;
            let _ = completion_tx
                .send(TaskMessage::Completed {
                    id: completion_id,
                    task_id,
                    outcome,
                })
                .await;
        })
        .detach();

        let timeout_tx = self.task_tx.clone();
        let timeout = Self::provider_refresh_timeout();
        smol::spawn(async move {
            smol::Timer::after(timeout).await;
            let _ = timeout_tx
                .send(TaskMessage::TimedOut {
                    id,
                    task_id,
                    reason,
                })
                .await;
        })
        .detach();
    }

    async fn start_refreshes(&mut self, ids: Vec<ProviderId>, reason: RefreshReason) {
        for id in ids {
            self.start_refresh(id, reason).await;
        }
    }

    async fn handle_task_message(&mut self, message: TaskMessage) {
        match message {
            TaskMessage::TimedOut {
                id,
                task_id,
                reason,
            } => {
                let should_report = self
                    .active_refreshes
                    .get_mut(&id)
                    .filter(|active| active.task_id == task_id)
                    .is_some_and(|active| {
                        if active.result_reported {
                            false
                        } else {
                            active.result_reported = true;
                            true
                        }
                    });
                if !should_report {
                    return;
                }

                log::warn!(
                    target: "refresh",
                    "provider {} refresh timed out after {:?} ({:?}); underlying task remains single-flight",
                    id,
                    Self::provider_refresh_timeout(),
                    reason
                );
                let outcome = Self::build_outcome(id, Err(ProviderError::Timeout));
                let _ = self.event_tx.send(RefreshEvent::Finished(outcome)).await;
            }
            TaskMessage::Completed {
                id,
                task_id,
                outcome,
            } => {
                let Some(active) = self.active_refreshes.get(&id) else {
                    return;
                };
                if active.task_id != task_id {
                    return;
                }
                let active = self
                    .active_refreshes
                    .remove(&id)
                    .expect("active refresh checked above");
                self.scheduler.clear_in_flight(&id);

                if active.result_reported {
                    return;
                }
                if active.generation != self.config_generation
                    || !self.scheduler.enabled_providers().contains(&id)
                {
                    self.emit_finished(id, RefreshResult::SkippedStale).await;
                    return;
                }
                if matches!(outcome.result, RefreshResult::Success { .. }) {
                    self.scheduler.record_success(&id);
                }
                let _ = self.event_tx.send(RefreshEvent::Finished(outcome)).await;
            }
        }
    }

    /// 配置或 registry 变化时立即让前台退出 Refreshing；底层任务仍保持 single-flight。
    async fn invalidate_active_refreshes(&mut self, enabled: Option<&[ProviderId]>) {
        let mut invalidated = Vec::new();
        for (id, active) in &mut self.active_refreshes {
            if active.result_reported {
                continue;
            }
            active.result_reported = true;
            let result = if enabled.is_some_and(|ids| !ids.contains(id)) {
                RefreshResult::SkippedDisabled
            } else {
                RefreshResult::SkippedStale
            };
            invalidated.push((id.clone(), result));
        }
        for (id, result) in invalidated {
            self.emit_finished(id, result).await;
        }
    }

    // ========================================================================
    // 控制请求
    // ========================================================================

    async fn handle_request(&mut self, request: RefreshRequest) -> bool {
        match request {
            RefreshRequest::RefreshAll { ids, reason } => {
                log::info!(target: "refresh", "refresh all requested ({:?})", reason);
                self.start_refreshes(ids, reason).await;
                if matches!(reason, RefreshReason::Manual) {
                    self.scheduler.advance_periodic_deadline();
                }
            }
            RefreshRequest::RefreshOne { id, reason } => {
                log::info!(target: "refresh", "refresh one requested: {} ({:?})", id, reason);
                self.start_refresh(id, reason).await;
            }
            RefreshRequest::UpdateConfig {
                interval_mins,
                enabled,
                provider_credentials,
            } => {
                let refresh_inputs_changed = self.provider_credentials != provider_credentials
                    || self.scheduler.enabled_providers() != enabled.as_slice();
                self.provider_credentials = provider_credentials;
                self.scheduler.update_config(interval_mins, enabled.clone());
                if refresh_inputs_changed {
                    self.config_generation = self.config_generation.wrapping_add(1);
                    self.invalidate_active_refreshes(Some(&enabled)).await;
                }
            }
            RefreshRequest::ReloadProviders => {
                log::info!(target: "refresh", "reloading custom providers");
                self.config_generation = self.config_generation.wrapping_add(1);
                self.invalidate_active_refreshes(None).await;

                let new_manager = Arc::new(crate::providers::ProviderManager::load_default());
                let statuses = new_manager.initial_statuses();
                let new_ids: std::collections::HashSet<_> =
                    statuses.iter().map(|status| &status.provider_id).collect();
                self.scheduler.cleanup_stale(&new_ids);
                self.manager.replace(new_manager);

                let _ = self
                    .event_tx
                    .send(RefreshEvent::ProvidersReloaded { statuses })
                    .await;
                log::info!(target: "refresh", "custom providers reloaded");
            }
            RefreshRequest::Shutdown => {
                log::info!(target: "refresh", "coordinator shutting down");
                return false;
            }
        }
        true
    }

    async fn handle_periodic(&mut self) {
        if self.scheduler.is_auto_refresh_disabled() {
            self.scheduler.advance_disabled_deadline();
            return;
        }
        log::info!(
            target: "refresh",
            "periodic refresh triggered (every {} min)",
            self.scheduler.interval_mins()
        );
        let ids = self.scheduler.enabled_providers().to_vec();
        self.start_refreshes(ids, RefreshReason::Periodic).await;
        self.scheduler.advance_periodic_deadline();
    }

    // ========================================================================
    // 事件循环
    // ========================================================================

    /// 主循环不等待 Provider 完成，因此刷新期间仍可处理配置、reload 和 shutdown。
    pub async fn run(mut self) {
        log::info!(target: "refresh", "coordinator started");

        loop {
            let wait = self.scheduler.time_until_next_periodic();
            let event = smol::future::or(
                async { LoopEvent::Request(self.request_rx.recv().await) },
                smol::future::or(
                    async { LoopEvent::Task(self.task_rx.recv().await) },
                    async {
                        smol::Timer::after(wait).await;
                        LoopEvent::Periodic
                    },
                ),
            )
            .await;

            match event {
                LoopEvent::Request(Ok(request)) => {
                    if !self.handle_request(request).await {
                        break;
                    }
                }
                LoopEvent::Request(Err(_)) => {
                    log::info!(target: "refresh", "request channel closed, shutting down");
                    break;
                }
                LoopEvent::Task(Ok(message)) => self.handle_task_message(message).await,
                LoopEvent::Task(Err(_)) => {
                    log::error!(target: "refresh", "internal task channel closed");
                    break;
                }
                LoopEvent::Periodic => self.handle_periodic().await,
            }
        }
    }
}

#[cfg(test)]
#[path = "coordinator_tests.rs"]
mod tests;
