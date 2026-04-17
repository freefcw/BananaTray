//! 刷新协调器 — 事件循环 + 并发执行。
//!
//! `RefreshCoordinator` 负责：
//! 1. 主事件循环：监听请求通道和周期定时器
//! 2. 并发刷新执行：线程池 + 结果收集
//! 3. 结果转换：ProviderError → RefreshOutcome
//!
//! 所有调度决策（cooldown、eligibility、deadline）委托给 `RefreshScheduler`。

use std::sync::Arc;
use std::time::Duration;

use smol::channel::{Receiver, Sender};

use crate::models::ProviderId;
use crate::providers::error_presenter::ProviderErrorPresenter;
use crate::providers::{ProviderManager, ProviderManagerHandle};

use super::scheduler::RefreshScheduler;
use super::types::*;

pub struct RefreshCoordinator {
    manager: ProviderManagerHandle,
    request_tx: Sender<RefreshRequest>,
    request_rx: Receiver<RefreshRequest>,
    event_tx: Sender<RefreshEvent>,
    scheduler: RefreshScheduler,
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
        let (request_tx, request_rx) = smol::channel::bounded(32);
        Self {
            manager,
            request_tx,
            request_rx,
            event_tx,
            scheduler: RefreshScheduler::new(),
        }
    }

    /// Get a sender to send requests to this coordinator
    pub fn sender(&self) -> Sender<RefreshRequest> {
        self.request_tx.clone()
    }

    // ========================================================================
    // 结果转换
    // ========================================================================

    /// Convert a provider refresh `Result` into a `RefreshOutcome` (pure, no side-effects).
    fn build_outcome(
        id: ProviderId,
        result: anyhow::Result<crate::models::RefreshData>,
    ) -> RefreshOutcome {
        match result {
            Ok(data) => {
                log::info!(target: "refresh", "provider {} refreshed: {} quotas", id, data.quotas.len());
                RefreshOutcome {
                    id,
                    result: RefreshResult::Success { data },
                }
            }
            Err(err) => {
                let classified = crate::providers::ProviderError::classify(&err);
                match &classified {
                    crate::providers::ProviderError::Unavailable { .. } => {
                        log::info!(target: "refresh", "provider {} unavailable: {}", id, classified);
                        RefreshOutcome {
                            id,
                            result: RefreshResult::Unavailable {
                                message: ProviderErrorPresenter::to_message(&classified),
                            },
                        }
                    }
                    _ => {
                        log::warn!(target: "refresh", "provider {} failed: {}", id, classified);
                        let error_kind = ProviderErrorPresenter::to_error_kind(&classified);
                        RefreshOutcome {
                            id,
                            result: RefreshResult::Failed {
                                error: ProviderErrorPresenter::to_message(&classified),
                                error_kind,
                            },
                        }
                    }
                }
            }
        }
    }

    // ========================================================================
    // 事件发送
    // ========================================================================

    /// Send a skip event for an ineligible provider.
    async fn send_skip(&self, id: ProviderId, result: RefreshResult) {
        let _ = self
            .event_tx
            .send(RefreshEvent::Finished(RefreshOutcome { id, result }))
            .await;
    }

    /// Record a completed outcome: clear in-flight, update last_refreshed, emit event.
    async fn record_outcome(&mut self, outcome: RefreshOutcome) {
        let id = outcome.id.clone();
        self.scheduler.clear_in_flight(&id);
        if matches!(outcome.result, RefreshResult::Success { .. }) {
            self.scheduler.record_success(&id);
        }
        let _ = self.event_tx.send(RefreshEvent::Finished(outcome)).await;
    }

    /// Mark a provider as in-flight and notify UI.
    async fn begin_refresh(&mut self, id: &ProviderId) {
        self.scheduler.mark_in_flight(id);
        let _ = self
            .event_tx
            .send(RefreshEvent::Started { id: id.clone() })
            .await;
    }

    // ========================================================================
    // 刷新执行
    // ========================================================================

    /// Run a single provider refresh on the blocking thread pool, catching panics.
    /// Panics are converted to `RefreshResult::Failed` so in-flight state is always cleared.
    async fn run_refresh(mgr: Arc<ProviderManager>, id: ProviderId) -> RefreshOutcome {
        smol::unblock(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                smol::block_on(mgr.refresh_by_id(&id))
            }))
            .unwrap_or_else(|_| {
                log::error!(target: "refresh", "provider {} panicked during refresh", id);
                Err(anyhow::anyhow!("provider panicked"))
            });
            RefreshCoordinator::build_outcome(id, result)
        })
        .await
    }

    /// Run a refresh with a coordinator-side timeout guard.
    ///
    /// This is the last-resort protection when a provider blocks inside CLI / HTTP / parser code.
    /// The underlying blocking task may continue running until its own I/O timeout fires, but the
    /// coordinator will stop waiting and clear in-flight state so future refreshes are not wedged.
    async fn run_refresh_with_timeout(
        mgr: Arc<ProviderManager>,
        id: ProviderId,
        reason: RefreshReason,
    ) -> RefreshOutcome {
        let timeout = Self::provider_refresh_timeout();
        let timeout_id = id.clone();
        smol::future::or(Self::run_refresh(mgr, id), async move {
            smol::Timer::after(timeout).await;
            log::warn!(
                target: "refresh",
                "provider {} refresh timed out after {:?} ({:?})",
                timeout_id,
                timeout,
                reason
            );
            Self::build_outcome(
                timeout_id,
                Err(crate::providers::ProviderError::Timeout.into()),
            )
        })
        .await
    }

    /// Refresh a single provider (used by RefreshOne).
    async fn execute_refresh(&mut self, id: ProviderId, reason: RefreshReason) {
        if let Some(skip) = self.scheduler.check_eligibility(&id, reason) {
            self.send_skip(id, skip).await;
            return;
        }

        self.begin_refresh(&id).await;
        let outcome = Self::run_refresh_with_timeout(self.manager.snapshot(), id, reason).await;
        self.record_outcome(outcome).await;
    }

    /// Refresh multiple providers concurrently.
    /// Sends Started events for all eligible providers upfront, then executes
    /// network requests on the smol blocking pool, collecting results in
    /// completion order via a channel.
    async fn execute_refresh_concurrent(&mut self, ids: Vec<ProviderId>, reason: RefreshReason) {
        // Phase 1: Filter eligible providers, send Started events
        let mut to_refresh = Vec::new();
        for id in ids {
            if let Some(skip) = self.scheduler.check_eligibility(&id, reason) {
                self.send_skip(id, skip).await;
                continue;
            }
            self.begin_refresh(&id).await;
            to_refresh.push(id);
        }

        if to_refresh.is_empty() {
            return;
        }

        // Phase 2: Spawn concurrent refresh tasks via smol thread pool.
        // 内部结果仍按完成顺序回传，避免慢 provider 阻塞已完成 provider 的状态清理/事件上报。
        let (result_tx, result_rx) = smol::channel::bounded::<RefreshOutcome>(to_refresh.len());
        for id in to_refresh {
            let mgr = self.manager.snapshot();
            let tx = result_tx.clone();
            smol::spawn(async move {
                let _ = tx
                    .send(Self::run_refresh_with_timeout(mgr, id, reason).await)
                    .await;
            })
            .detach();
        }
        drop(result_tx);

        // Phase 3: Collect results as they arrive
        while let Ok(outcome) = result_rx.recv().await {
            self.record_outcome(outcome).await;
        }
    }

    // ========================================================================
    // 事件循环
    // ========================================================================

    /// Run the coordinator event loop. This is the main entry point.
    /// It processes requests and runs periodic refresh in a single loop.
    pub async fn run(mut self) {
        log::info!(target: "refresh", "coordinator started");

        loop {
            // 使用绝对 deadline 计算剩余等待时间，避免收到请求时定时器被重置
            let wait = self.scheduler.time_until_next_periodic();

            // Wait for either a request or the periodic timer
            let request = smol::future::or(async { Some(self.request_rx.recv().await) }, async {
                smol::Timer::after(wait).await;
                None
            })
            .await;

            match request {
                // Timer fired — periodic refresh
                None => {
                    if self.scheduler.is_auto_refresh_disabled() {
                        self.scheduler.advance_disabled_deadline();
                        continue;
                    }
                    log::info!(target: "refresh", "periodic refresh triggered (every {} min)", self.scheduler.interval_mins());
                    let kinds: Vec<_> = self.scheduler.enabled_providers().to_vec();
                    self.execute_refresh_concurrent(kinds, RefreshReason::Periodic)
                        .await;
                    self.scheduler.advance_periodic_deadline();
                }
                // Request received
                Some(Ok(req)) => match req {
                    RefreshRequest::RefreshAll { reason } => {
                        log::info!(target: "refresh", "refresh all requested ({:?})", reason);
                        let kinds: Vec<_> = self.scheduler.enabled_providers().to_vec();
                        self.execute_refresh_concurrent(kinds, reason).await;
                        // 手动触发全部刷新后重置周期定时器，避免短时间内重复刷新
                        if matches!(reason, RefreshReason::Manual) {
                            self.scheduler.advance_periodic_deadline();
                        }
                    }
                    RefreshRequest::RefreshOne { id, reason } => {
                        log::info!(target: "refresh", "refresh one requested: {} ({:?})", id, reason);
                        self.execute_refresh(id, reason).await;
                    }
                    RefreshRequest::UpdateConfig {
                        interval_mins,
                        enabled,
                    } => {
                        self.scheduler.update_config(interval_mins, enabled);
                    }
                    RefreshRequest::ReloadProviders => {
                        log::info!(target: "refresh", "reloading custom providers");
                        let new_manager = Arc::new(crate::providers::ProviderManager::new());
                        let statuses = new_manager.initial_statuses();

                        // 清理已不存在的 provider 的残留状态
                        let new_ids: std::collections::HashSet<_> =
                            statuses.iter().map(|s| &s.provider_id).collect();
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
                        break;
                    }
                },
                // Channel closed
                Some(Err(_)) => {
                    log::info!(target: "refresh", "request channel closed, shutting down");
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "coordinator_tests.rs"]
mod tests;
