use super::*;
use crate::models::ErrorKind;
use crate::models::{
    FailureAdvice, ProviderDescriptor, ProviderId, ProviderKind, ProviderMetadata, RefreshData,
};
use crate::providers::{
    AiProvider, ProviderCapabilities, ProviderError, ProviderExecutionContext, ProviderManager,
    ProviderManagerHandle, ProviderResult,
};
use async_trait::async_trait;
use std::borrow::Cow;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

// ============================================================================
// ProviderError 分类测试（build_outcome 使用的错误转换）
// ============================================================================

#[test]
fn test_classify_error_kind_config_missing() {
    let error = ProviderError::ConfigMissing {
        key: "github_token".to_string(),
    };
    assert_eq!(error.error_kind(), ErrorKind::ConfigMissing);
}

#[test]
fn test_classify_error_kind_auth_required() {
    let error = ProviderError::AuthRequired { advice: None };
    assert_eq!(error.error_kind(), ErrorKind::AuthRequired);
}

#[test]
fn test_classify_error_kind_session_expired() {
    let error = ProviderError::SessionExpired {
        advice: Some(FailureAdvice::ReloginCli {
            cli: "test-cli".to_string(),
        }),
    };
    assert_eq!(error.error_kind(), ErrorKind::AuthRequired);
}

#[test]
fn test_classify_error_kind_network_error() {
    assert_eq!(ProviderError::Timeout.error_kind(), ErrorKind::NetworkError);
    assert_eq!(
        ProviderError::NetworkFailed {
            reason: "timeout".to_string(),
        }
        .error_kind(),
        ErrorKind::NetworkError
    );
}

#[test]
fn test_classify_error_kind_unknown() {
    let error = ProviderError::CliNotFound {
        cli_name: "claude".to_string(),
    };
    assert_eq!(error.error_kind(), ErrorKind::Unknown);
}

struct DelayedProvider {
    id: String,
    delay: Duration,
    active: Arc<AtomicUsize>,
    max_active: Arc<AtomicUsize>,
}

impl DelayedProvider {
    fn new(id: &str, delay: Duration) -> Self {
        Self {
            id: id.to_string(),
            delay,
            active: Arc::new(AtomicUsize::new(0)),
            max_active: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn with_counters(
        id: &str,
        delay: Duration,
        active: Arc<AtomicUsize>,
        max_active: Arc<AtomicUsize>,
    ) -> Self {
        Self {
            id: id.to_string(),
            delay,
            active,
            max_active,
        }
    }
}

#[async_trait]
impl AiProvider for DelayedProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: Cow::Owned(self.id.clone()),
            metadata: ProviderMetadata {
                kind: ProviderKind::Custom,
                display_name: self.id.clone(),
                brand_name: self.id.clone(),
                icon_asset: String::new(),
                dashboard_url: String::new(),
                account_hint: String::new(),
                source_label: "test".to_string(),
            },
        }
    }

    async fn refresh(&self, _ctx: &ProviderExecutionContext<'_>) -> ProviderResult<RefreshData> {
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_active.fetch_max(active, Ordering::SeqCst);
        std::thread::sleep(self.delay);
        self.active.fetch_sub(1, Ordering::SeqCst);
        Ok(RefreshData::quotas_only(Vec::new()))
    }
}

impl ProviderCapabilities for DelayedProvider {}

async fn drive_until_idle(coordinator: &mut RefreshCoordinator) {
    while !coordinator.active_refreshes.is_empty() {
        let message = coordinator.task_rx.recv().await.unwrap();
        coordinator.handle_task_message(message).await;
    }
}

fn drain_events(event_rx: &smol::channel::Receiver<RefreshEvent>) -> Vec<RefreshEvent> {
    let mut events = Vec::new();
    while let Ok(event) = event_rx.try_recv() {
        events.push(event);
    }
    events
}

#[test]
fn test_refreshes_report_physical_completion_order() {
    smol::block_on(async {
        let mut manager = ProviderManager::new();
        let slow_id = ProviderId::Custom("test:slow".to_string());
        let fast_id = ProviderId::Custom("test:fast".to_string());
        manager.register(Arc::new(DelayedProvider::new(
            "test:slow",
            Duration::from_millis(50),
        )));
        manager.register(Arc::new(DelayedProvider::new(
            "test:fast",
            Duration::from_millis(5),
        )));

        let (event_tx, event_rx) = smol::channel::bounded(8);
        let mut coordinator =
            RefreshCoordinator::new(ProviderManagerHandle::new(manager), event_tx);
        coordinator
            .scheduler
            .update_config(10, vec![slow_id.clone(), fast_id.clone()]);

        coordinator
            .start_refreshes(
                vec![slow_id.clone(), fast_id.clone()],
                RefreshReason::Manual,
            )
            .await;
        drive_until_idle(&mut coordinator).await;

        let mut finished_ids = Vec::new();
        while let Ok(event) = event_rx.try_recv() {
            if let RefreshEvent::Finished(outcome) = event {
                finished_ids.push(outcome.id);
            }
        }
        assert_eq!(finished_ids, vec![fast_id, slow_id]);
    });
}

#[test]
fn test_timeout_keeps_single_flight_until_underlying_task_finishes() {
    smol::block_on(async {
        let id = ProviderId::Custom("test:timeout".to_string());
        let active = Arc::new(AtomicUsize::new(0));
        let max_active = Arc::new(AtomicUsize::new(0));
        let mut manager = ProviderManager::new();
        manager.register(Arc::new(DelayedProvider::with_counters(
            "test:timeout",
            Duration::from_millis(250),
            active,
            max_active.clone(),
        )));

        let (event_tx, event_rx) = smol::channel::bounded(16);
        let mut coordinator =
            RefreshCoordinator::new(ProviderManagerHandle::new(manager), event_tx);
        coordinator.scheduler.update_config(10, vec![id.clone()]);

        coordinator
            .start_refresh(id.clone(), RefreshReason::Manual)
            .await;
        let timeout = coordinator.task_rx.recv().await.unwrap();
        coordinator.handle_task_message(timeout).await;

        coordinator
            .start_refresh(id.clone(), RefreshReason::Manual)
            .await;

        let mut saw_timeout = false;
        let mut saw_in_flight_skip = false;
        while let Ok(event) = event_rx.try_recv() {
            if let RefreshEvent::Finished(outcome) = event {
                saw_timeout |= matches!(
                    outcome.result,
                    RefreshResult::Failed {
                        error_kind: ErrorKind::NetworkError,
                        ..
                    }
                );
                saw_in_flight_skip |= matches!(outcome.result, RefreshResult::SkippedInFlight);
            }
        }
        assert!(saw_timeout);
        assert!(saw_in_flight_skip);

        drive_until_idle(&mut coordinator).await;
        assert_eq!(max_active.load(Ordering::SeqCst), 1);

        coordinator
            .start_refresh(id.clone(), RefreshReason::Manual)
            .await;
        assert!(coordinator.active_refreshes.contains_key(&id));
    });
}

struct PanicProvider {
    id: String,
}

#[async_trait]
impl AiProvider for PanicProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: Cow::Owned(self.id.clone()),
            metadata: ProviderMetadata {
                kind: ProviderKind::Custom,
                display_name: self.id.clone(),
                brand_name: self.id.clone(),
                icon_asset: String::new(),
                dashboard_url: String::new(),
                account_hint: String::new(),
                source_label: "test".to_string(),
            },
        }
    }

    async fn refresh(&self, _ctx: &ProviderExecutionContext<'_>) -> ProviderResult<RefreshData> {
        panic!("simulated provider panic");
    }
}

impl ProviderCapabilities for PanicProvider {}

#[test]
fn test_panic_releases_single_flight_after_task_completion() {
    smol::block_on(async {
        let id = ProviderId::Custom("test:panic".to_string());
        let mut manager = ProviderManager::new();
        manager.register(Arc::new(PanicProvider {
            id: "test:panic".to_string(),
        }));

        let (event_tx, event_rx) = smol::channel::bounded(8);
        let mut coordinator =
            RefreshCoordinator::new(ProviderManagerHandle::new(manager), event_tx);
        coordinator.scheduler.update_config(10, vec![id.clone()]);

        coordinator
            .start_refresh(id.clone(), RefreshReason::Manual)
            .await;
        drive_until_idle(&mut coordinator).await;

        let outcome = drain_events(&event_rx)
            .into_iter()
            .find_map(|event| match event {
                RefreshEvent::Finished(outcome) => Some(outcome),
                _ => None,
            })
            .expect("panic outcome");
        assert!(matches!(outcome.result, RefreshResult::Failed { .. }));

        coordinator
            .start_refresh(id.clone(), RefreshReason::Manual)
            .await;
        assert!(coordinator.active_refreshes.contains_key(&id));
    });
}

#[test]
fn test_config_change_discards_old_result_and_releases_ui_immediately() {
    smol::block_on(async {
        let id = ProviderId::Custom("test:stale".to_string());
        let mut manager = ProviderManager::new();
        manager.register(Arc::new(DelayedProvider::new(
            "test:stale",
            Duration::from_millis(180),
        )));

        let (event_tx, event_rx) = smol::channel::bounded(16);
        let mut coordinator =
            RefreshCoordinator::new(ProviderManagerHandle::new(manager), event_tx);
        coordinator.scheduler.update_config(10, vec![id.clone()]);

        coordinator
            .start_refresh(id.clone(), RefreshReason::Manual)
            .await;
        let mut credentials = crate::models::ProviderSettings::default();
        credentials.set_credential("test_token", "new-token".to_string());
        coordinator
            .handle_request(RefreshRequest::UpdateConfig {
                interval_mins: 10,
                enabled: vec![id.clone()],
                provider_credentials: credentials,
            })
            .await;

        let events_before_completion = drain_events(&event_rx);
        assert!(events_before_completion.iter().any(|event| matches!(
            event,
            RefreshEvent::Finished(RefreshOutcome {
                result: RefreshResult::SkippedStale,
                ..
            })
        )));

        drive_until_idle(&mut coordinator).await;
        assert!(drain_events(&event_rx).into_iter().all(|event| !matches!(
            event,
            RefreshEvent::Finished(RefreshOutcome {
                result: RefreshResult::Success { .. },
                ..
            })
        )));
    });
}

#[test]
fn test_shutdown_is_processed_while_provider_is_still_running() {
    smol::block_on(async {
        let id = ProviderId::Custom("test:shutdown".to_string());
        let mut manager = ProviderManager::new();
        manager.register(Arc::new(DelayedProvider::new(
            "test:shutdown",
            Duration::from_millis(500),
        )));

        let (event_tx, event_rx) = smol::channel::bounded(8);
        let coordinator = RefreshCoordinator::new(ProviderManagerHandle::new(manager), event_tx);
        let request_tx = coordinator.sender();
        let task = smol::spawn(coordinator.run());

        request_tx
            .send(RefreshRequest::UpdateConfig {
                interval_mins: 10,
                enabled: vec![id.clone()],
                provider_credentials: crate::models::ProviderSettings::default(),
            })
            .await
            .unwrap();
        request_tx
            .send(RefreshRequest::RefreshOne {
                id,
                reason: RefreshReason::Manual,
            })
            .await
            .unwrap();
        assert!(matches!(
            event_rx.recv().await.unwrap(),
            RefreshEvent::Started { .. }
        ));

        let started = Instant::now();
        request_tx.send(RefreshRequest::Shutdown).await.unwrap();
        task.await;
        assert!(
            started.elapsed() < Duration::from_millis(250),
            "shutdown waited for provider completion"
        );
    });
}

#[test]
fn test_reload_providers_replaces_shared_manager_snapshot() {
    smol::block_on(async {
        let (event_tx, event_rx) = smol::channel::bounded(8);
        let manager = ProviderManagerHandle::default();
        let initial = manager.snapshot();
        let coordinator = RefreshCoordinator::new(manager.clone(), event_tx);
        let request_tx = coordinator.sender();

        let task = smol::spawn(coordinator.run());
        request_tx
            .send(RefreshRequest::ReloadProviders)
            .await
            .unwrap();

        match event_rx.recv().await.unwrap() {
            RefreshEvent::ProvidersReloaded { statuses } => assert!(!statuses.is_empty()),
            other => panic!("unexpected refresh event: {other:?}"),
        }

        let reloaded = manager.snapshot();
        assert!(!Arc::ptr_eq(&initial, &reloaded));

        request_tx.send(RefreshRequest::Shutdown).await.unwrap();
        task.await;
    });
}
