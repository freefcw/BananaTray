//! 刷新协调器 — 事件循环 + 并发执行。
//!
//! `RefreshCoordinator` 负责：
//! 1. 主事件循环：监听请求通道和周期定时器
//! 2. 并发刷新执行：线程池 + 结果收集
//! 3. 结果转换：ProviderError → RefreshOutcome
//!
//! 所有调度决策（cooldown、eligibility、deadline）委托给 `RefreshScheduler`。

use std::sync::Arc;

use smol::channel::{Receiver, Sender};

use crate::models::ProviderId;
use crate::providers::error_presenter::ProviderErrorPresenter;
use crate::providers::ProviderManager;

use super::scheduler::RefreshScheduler;
use super::types::*;

pub struct RefreshCoordinator {
    manager: Arc<ProviderManager>,
    request_tx: Sender<RefreshRequest>,
    request_rx: Receiver<RefreshRequest>,
    event_tx: Sender<RefreshEvent>,
    scheduler: RefreshScheduler,
}

impl RefreshCoordinator {
    pub fn new(manager: Arc<ProviderManager>, event_tx: Sender<RefreshEvent>) -> Self {
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

    /// Refresh a single provider (used by RefreshOne).
    async fn execute_refresh(&mut self, id: ProviderId, reason: RefreshReason) {
        if let Some(skip) = self.scheduler.check_eligibility(&id, reason) {
            self.send_skip(id, skip).await;
            return;
        }

        self.begin_refresh(&id).await;

        let mgr = self.manager.clone();
        let id_clone = id.clone();
        let result = smol::unblock(move || smol::block_on(mgr.refresh_by_id(&id_clone))).await;
        self.record_outcome(Self::build_outcome(id, result)).await;
    }

    /// Refresh multiple providers concurrently.
    /// Sends Started events for all eligible providers upfront, then executes
    /// network requests in parallel threads, collecting results via a channel.
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

        // Phase 2: Spawn concurrent refresh threads
        let (result_tx, result_rx) = smol::channel::bounded::<RefreshOutcome>(to_refresh.len());

        for id in &to_refresh {
            let mgr = self.manager.clone();
            let tx = result_tx.clone();
            let id = id.clone();
            std::thread::spawn(move || {
                let result = smol::block_on(mgr.refresh_by_id(&id));
                let outcome = RefreshCoordinator::build_outcome(id, result);
                let _ = smol::block_on(tx.send(outcome));
            });
        }
        drop(result_tx);

        // Phase 3: Collect results as they arrive
        let expected = to_refresh.len();
        for _ in 0..expected {
            match result_rx.recv().await {
                Ok(outcome) => self.record_outcome(outcome).await,
                Err(_) => break,
            }
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

                        self.manager = new_manager;

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
