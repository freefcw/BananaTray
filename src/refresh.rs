use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use smol::channel::{Receiver, Sender};

use crate::models::{ProviderKind, QuotaInfo};
use crate::providers::ProviderManager;

// ============================================================================
// 消息类型
// ============================================================================

/// 刷新触发原因
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum RefreshReason {
    Startup,
    Periodic,
    Manual,
    ProviderToggled,
}

/// 发送给协调器的请求
#[derive(Debug)]
#[allow(dead_code)]
pub enum RefreshRequest {
    RefreshAll {
        reason: RefreshReason,
    },
    RefreshOne {
        kind: ProviderKind,
        reason: RefreshReason,
    },
    UpdateConfig {
        interval_mins: u64,
        enabled: Vec<ProviderKind>,
    },
    Shutdown,
}

/// 协调器发出的事件
#[derive(Debug)]
pub enum RefreshEvent {
    Started { kind: ProviderKind },
    Finished(RefreshOutcome),
}

/// 单个 Provider 的刷新结果
#[derive(Debug)]
pub struct RefreshOutcome {
    pub kind: ProviderKind,
    pub result: RefreshResult,
}

/// 刷新结果类型
#[derive(Debug)]
pub enum RefreshResult {
    Success { quotas: Vec<QuotaInfo> },
    Unavailable { message: String },
    Failed { error: String },
    SkippedCooldown,
    SkippedInFlight,
    SkippedDisabled,
}

// ============================================================================
// RefreshCoordinator
// ============================================================================

pub struct RefreshCoordinator {
    manager: Arc<ProviderManager>,
    request_tx: Sender<RefreshRequest>,
    request_rx: Receiver<RefreshRequest>,
    event_tx: Sender<RefreshEvent>,
    /// Per-provider last successful refresh instant
    last_refreshed: HashMap<ProviderKind, Instant>,
    /// Per-provider in-flight flag
    in_flight: HashMap<ProviderKind, bool>,
    /// Current config
    interval_mins: u64,
    enabled_providers: Vec<ProviderKind>,
}

/// 最小 cooldown 时间（秒），防止过于频繁的刷新
const MIN_COOLDOWN_SECS: u64 = 30;
/// 自动刷新禁用时的检查间隔（秒）
const DISABLED_CHECK_INTERVAL_SECS: u64 = 3600;

impl RefreshCoordinator {
    pub fn new(manager: Arc<ProviderManager>, event_tx: Sender<RefreshEvent>) -> Self {
        let (request_tx, request_rx) = smol::channel::bounded(32);
        Self {
            manager,
            request_tx,
            request_rx,
            event_tx,
            last_refreshed: HashMap::new(),
            in_flight: HashMap::new(),
            interval_mins: 5,
            enabled_providers: Vec::new(),
        }
    }

    /// Get a sender to send requests to this coordinator
    pub fn sender(&self) -> Sender<RefreshRequest> {
        self.request_tx.clone()
    }

    /// Compute cooldown duration: half the interval, minimum MIN_COOLDOWN_SECS
    fn cooldown(&self) -> Duration {
        let interval_secs = self.interval_mins * 60;
        let half = interval_secs / 2;
        Duration::from_secs(half.max(MIN_COOLDOWN_SECS))
    }

    /// Check if a provider was recently refreshed (within cooldown)
    fn is_on_cooldown(&self, kind: ProviderKind) -> bool {
        if let Some(instant) = self.last_refreshed.get(&kind) {
            instant.elapsed() < self.cooldown()
        } else {
            false
        }
    }

    /// Check if a provider is currently being refreshed
    fn is_in_flight(&self, kind: ProviderKind) -> bool {
        self.in_flight.get(&kind).copied().unwrap_or(false)
    }

    /// Check if a provider is eligible for refresh, returning the skip reason if not.
    fn check_eligibility(
        &self,
        kind: ProviderKind,
        reason: RefreshReason,
    ) -> Option<RefreshResult> {
        if !self.enabled_providers.contains(&kind) {
            return Some(RefreshResult::SkippedDisabled);
        }
        if self.is_in_flight(kind) {
            return Some(RefreshResult::SkippedInFlight);
        }
        if matches!(reason, RefreshReason::Periodic | RefreshReason::Startup)
            && self.is_on_cooldown(kind)
        {
            log::info!(target: "refresh", "skipping {:?} (cooldown)", kind);
            return Some(RefreshResult::SkippedCooldown);
        }
        None
    }

    /// Send a skip event for an ineligible provider.
    async fn send_skip(&self, kind: ProviderKind, result: RefreshResult) {
        let _ = self
            .event_tx
            .send(RefreshEvent::Finished(RefreshOutcome { kind, result }))
            .await;
    }

    /// Convert a provider refresh `Result` into a `RefreshOutcome` (pure, no side-effects).
    fn build_outcome(kind: ProviderKind, result: anyhow::Result<Vec<QuotaInfo>>) -> RefreshOutcome {
        match result {
            Ok(quotas) => {
                log::info!(target: "refresh", "provider {:?} refreshed: {} quotas", kind, quotas.len());
                RefreshOutcome {
                    kind,
                    result: RefreshResult::Success { quotas },
                }
            }
            Err(err) => {
                let classified = crate::providers::ProviderError::classify(&err);
                match &classified {
                    crate::providers::ProviderError::Unavailable { .. } => {
                        log::info!(target: "refresh", "provider {:?} unavailable: {}", kind, classified);
                        RefreshOutcome {
                            kind,
                            result: RefreshResult::Unavailable {
                                message: classified.to_string(),
                            },
                        }
                    }
                    _ => {
                        log::warn!(target: "refresh", "provider {:?} failed: {}", kind, classified);
                        RefreshOutcome {
                            kind,
                            result: RefreshResult::Failed {
                                error: classified.to_string(),
                            },
                        }
                    }
                }
            }
        }
    }

    /// Record a completed outcome: clear in-flight, update last_refreshed, emit event.
    async fn record_outcome(&mut self, outcome: RefreshOutcome) {
        let kind = outcome.kind;
        self.in_flight.insert(kind, false);
        if matches!(outcome.result, RefreshResult::Success { .. }) {
            self.last_refreshed.insert(kind, Instant::now());
        }
        let _ = self.event_tx.send(RefreshEvent::Finished(outcome)).await;
    }

    /// Mark a provider as in-flight and notify UI.
    async fn begin_refresh(&mut self, kind: ProviderKind) {
        self.in_flight.insert(kind, true);
        let _ = self.event_tx.send(RefreshEvent::Started { kind }).await;
    }

    /// Refresh a single provider (used by RefreshOne).
    async fn execute_refresh(&mut self, kind: ProviderKind, reason: RefreshReason) {
        if let Some(skip) = self.check_eligibility(kind, reason) {
            self.send_skip(kind, skip).await;
            return;
        }

        self.begin_refresh(kind).await;

        let mgr = self.manager.clone();
        let result = smol::unblock(move || smol::block_on(mgr.refresh_provider(kind))).await;
        self.record_outcome(Self::build_outcome(kind, result)).await;
    }

    /// Refresh multiple providers concurrently.
    /// Sends Started events for all eligible providers upfront, then executes
    /// network requests in parallel threads, collecting results via a channel.
    async fn execute_refresh_concurrent(
        &mut self,
        kinds: Vec<ProviderKind>,
        reason: RefreshReason,
    ) {
        // Phase 1: Filter eligible providers, send Started events
        let mut to_refresh = Vec::new();
        for kind in kinds {
            if let Some(skip) = self.check_eligibility(kind, reason) {
                self.send_skip(kind, skip).await;
                continue;
            }
            self.begin_refresh(kind).await;
            to_refresh.push(kind);
        }

        if to_refresh.is_empty() {
            return;
        }

        // Phase 2: Spawn concurrent refresh threads
        let (result_tx, result_rx) = smol::channel::bounded::<RefreshOutcome>(to_refresh.len());

        for &kind in &to_refresh {
            let mgr = self.manager.clone();
            let tx = result_tx.clone();
            std::thread::spawn(move || {
                let result = smol::block_on(mgr.refresh_provider(kind));
                let outcome = RefreshCoordinator::build_outcome(kind, result);
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

    /// Run the coordinator event loop. This is the main entry point.
    /// It processes requests and runs periodic refresh in a single loop.
    pub async fn run(mut self) {
        log::info!(target: "refresh", "coordinator started");

        loop {
            // Calculate next periodic refresh time
            let interval = if self.interval_mins > 0 {
                Duration::from_secs(self.interval_mins * 60)
            } else {
                Duration::from_secs(DISABLED_CHECK_INTERVAL_SECS)
            };

            // Wait for either a request or the periodic timer
            let request = smol::future::or(async { Some(self.request_rx.recv().await) }, async {
                smol::Timer::after(interval).await;
                None
            })
            .await;

            match request {
                // Timer fired — periodic refresh
                None => {
                    if self.interval_mins == 0 {
                        continue; // auto-refresh disabled
                    }
                    log::info!(target: "refresh", "periodic refresh triggered (every {} min)", self.interval_mins);
                    let kinds: Vec<_> = self.enabled_providers.clone();
                    self.execute_refresh_concurrent(kinds, RefreshReason::Periodic)
                        .await;
                }
                // Request received
                Some(Ok(req)) => match req {
                    RefreshRequest::RefreshAll { reason } => {
                        log::info!(target: "refresh", "refresh all requested ({:?})", reason);
                        let kinds: Vec<_> = self.enabled_providers.clone();
                        self.execute_refresh_concurrent(kinds, reason).await;
                    }
                    RefreshRequest::RefreshOne { kind, reason } => {
                        log::info!(target: "refresh", "refresh one requested: {:?} ({:?})", kind, reason);
                        self.execute_refresh(kind, reason).await;
                    }
                    RefreshRequest::UpdateConfig {
                        interval_mins,
                        enabled,
                    } => {
                        log::info!(target: "refresh", "config updated: interval={}min, {} providers enabled", interval_mins, enabled.len());
                        self.interval_mins = interval_mins;
                        self.enabled_providers = enabled;
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
