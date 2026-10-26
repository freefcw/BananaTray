use super::*;

#[test]
fn refresh_worker_shutdown_stops_coordinator_before_deadline() {
    let manager = ProviderManagerHandle::new(ProviderManager::new());
    let (event_tx, _event_rx) = smol::channel::bounded(1);
    let coordinator = RefreshCoordinator::new(manager, event_tx);
    let worker = crate::refresh::RefreshWorker::spawn(coordinator).unwrap();

    assert!(worker.shutdown_before(std::time::Instant::now() + std::time::Duration::from_secs(1)));
}
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
use std::time::Duration;

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

struct GatedProvider {
    id: String,
    wait_rx: Option<smol::channel::Receiver<()>>,
    notify_tx: Option<smol::channel::Sender<()>>,
}

impl GatedProvider {
    fn new(
        id: &str,
        wait_rx: Option<smol::channel::Receiver<()>>,
        notify_tx: Option<smol::channel::Sender<()>>,
    ) -> Self {
        Self {
            id: id.to_string(),
            wait_rx,
            notify_tx,
        }
    }
}

#[async_trait]
impl AiProvider for GatedProvider {
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
        if let Some(rx) = &self.wait_rx {
            let _ = rx.recv().await;
            smol::Timer::after(Duration::from_millis(15)).await;
        }
        if let Some(tx) = &self.notify_tx {
            let _ = tx.send(()).await;
        }
        Ok(RefreshData::quotas_only(Vec::new()))
    }
}

impl ProviderCapabilities for GatedProvider {}

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
        let (gate_tx, gate_rx) = smol::channel::bounded(1);
        let mut manager = ProviderManager::new();
        let slow_id = ProviderId::Custom("test:slow".to_string());
        let fast_id = ProviderId::Custom("test:fast".to_string());
        manager.register(Arc::new(GatedProvider::new(
            "test:slow",
            Some(gate_rx),
            None,
        )));
        manager.register(Arc::new(GatedProvider::new(
            "test:fast",
            None,
            Some(gate_tx),
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

#[test]
fn test_config_change_after_timeout_reruns_when_old_task_finishes() {
    smol::block_on(async {
        let id = ProviderId::Custom("test:timeout-config".to_string());
        let mut manager = ProviderManager::new();
        manager.register(Arc::new(DelayedProvider::new(
            "test:timeout-config",
            Duration::from_millis(250),
        )));

        let (event_tx, _event_rx) = smol::channel::bounded(16);
        let mut coordinator =
            RefreshCoordinator::new(ProviderManagerHandle::new(manager), event_tx);
        coordinator.scheduler.update_config(10, vec![id.clone()]);

        coordinator
            .start_refresh(id.clone(), RefreshReason::Manual)
            .await;
        let timeout = coordinator.task_rx.recv().await.unwrap();
        assert!(matches!(&timeout, TaskMessage::TimedOut { .. }));
        coordinator.handle_task_message(timeout).await;
        let old_task_id = coordinator.active_refreshes[&id].task_id;

        let mut credentials = crate::models::ProviderSettings::default();
        credentials.set_credential("test_token", "new-token".to_string());
        coordinator
            .handle_request(RefreshRequest::UpdateConfig {
                interval_mins: 10,
                enabled: vec![id.clone()],
                provider_credentials: credentials,
            })
            .await;
        assert!(coordinator.active_refreshes[&id].rerun.is_some());

        let completion = coordinator.task_rx.recv().await.unwrap();
        assert!(matches!(&completion, TaskMessage::Completed { .. }));
        coordinator.handle_task_message(completion).await;

        assert_ne!(coordinator.active_refreshes[&id].task_id, old_task_id);
        drive_until_idle(&mut coordinator).await;
    });
}

#[test]
fn test_late_success_after_timeout_is_forwarded_and_records_success() {
    smol::block_on(async {
        let id = ProviderId::Custom("test:late".to_string());
        let mut manager = ProviderManager::new();
        manager.register(Arc::new(DelayedProvider::new(
            "test:late",
            // 超过测试环境 100ms 的 provider 超时，让 TimedOut 先于 Completed 到达
            Duration::from_millis(250),
        )));

        let (event_tx, event_rx) = smol::channel::bounded(16);
        let mut coordinator =
            RefreshCoordinator::new(ProviderManagerHandle::new(manager), event_tx);
        coordinator.scheduler.update_config(10, vec![id.clone()]);

        coordinator
            .start_refresh(id.clone(), RefreshReason::Manual)
            .await;
        drive_until_idle(&mut coordinator).await;

        let events = drain_events(&event_rx);
        assert!(events.iter().any(|event| matches!(
            event,
            RefreshEvent::Finished(RefreshOutcome {
                reason: Some(RefreshReason::Manual),
                result: RefreshResult::Failed {
                    error_kind: ErrorKind::NetworkError,
                    ..
                },
                ..
            })
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            RefreshEvent::Finished(RefreshOutcome {
                reason: Some(RefreshReason::Manual),
                result: RefreshResult::Success { .. },
                ..
            })
        )));
        assert!(
            coordinator.scheduler.is_on_cooldown(&id),
            "late success must still be recorded for cooldown"
        );
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
                reason: Some(RefreshReason::Manual),
                result: RefreshResult::SkippedStale,
                ..
            })
        )));

        // 旧结果被丢弃；旧任务结束后用新凭证补跑一次，补跑的成功才会上报。
        drive_until_idle(&mut coordinator).await;
        let events = drain_events(&event_rx);
        let started = events
            .iter()
            .filter(|event| matches!(event, RefreshEvent::Started { .. }))
            .count();
        let successes = events
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    RefreshEvent::Finished(RefreshOutcome {
                        reason: Some(RefreshReason::Manual),
                        result: RefreshResult::Success { .. },
                        ..
                    })
                )
            })
            .count();
        assert_eq!((started, successes), (1, 1));
    });
}

#[test]
fn test_request_during_invalidated_refresh_reruns_instead_of_skipping() {
    smol::block_on(async {
        let id = ProviderId::Custom("test:rerun".to_string());
        let active = Arc::new(AtomicUsize::new(0));
        let max_active = Arc::new(AtomicUsize::new(0));
        let mut manager = ProviderManager::new();
        manager.register(Arc::new(DelayedProvider::with_counters(
            "test:rerun",
            Duration::from_millis(120),
            active,
            max_active.clone(),
        )));

        let (event_tx, event_rx) = smol::channel::bounded(16);
        let mut coordinator =
            RefreshCoordinator::new(ProviderManagerHandle::new(manager), event_tx);
        coordinator.scheduler.update_config(10, vec![id.clone()]);

        coordinator
            .start_refresh(id.clone(), RefreshReason::Periodic)
            .await;
        // 刷新进行中把 provider 关掉再打开，前台随后补发单个刷新。
        for enabled in [vec![], vec![id.clone()]] {
            coordinator
                .handle_request(RefreshRequest::UpdateConfig {
                    interval_mins: 10,
                    enabled,
                    provider_credentials: crate::models::ProviderSettings::default(),
                })
                .await;
        }
        coordinator
            .handle_request(RefreshRequest::RefreshOne {
                id: id.clone(),
                reason: RefreshReason::ProviderToggled,
            })
            .await;

        drive_until_idle(&mut coordinator).await;

        let events = drain_events(&event_rx);
        assert!(!events.iter().any(|event| matches!(
            event,
            RefreshEvent::Finished(RefreshOutcome {
                result: RefreshResult::SkippedInFlight,
                ..
            })
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            RefreshEvent::Finished(RefreshOutcome {
                reason: Some(RefreshReason::ProviderToggled),
                result: RefreshResult::Success { .. },
                ..
            })
        )));
        assert_eq!(max_active.load(Ordering::SeqCst), 1);
    });
}

#[test]
fn test_reenabling_provider_after_invalidated_refresh_reruns_automatically() {
    smol::block_on(async {
        let id = ProviderId::Custom("test:reenable".to_string());
        let mut manager = ProviderManager::new();
        manager.register(Arc::new(DelayedProvider::new(
            "test:reenable",
            Duration::from_millis(120),
        )));

        let (event_tx, event_rx) = smol::channel::bounded(16);
        let mut coordinator =
            RefreshCoordinator::new(ProviderManagerHandle::new(manager), event_tx);
        coordinator.scheduler.update_config(10, vec![id.clone()]);

        coordinator
            .start_refresh(id.clone(), RefreshReason::Periodic)
            .await;
        for enabled in [vec![], vec![id.clone()]] {
            coordinator
                .handle_request(RefreshRequest::UpdateConfig {
                    interval_mins: 10,
                    enabled,
                    provider_credentials: crate::models::ProviderSettings::default(),
                })
                .await;
        }

        drive_until_idle(&mut coordinator).await;

        let events = drain_events(&event_rx);
        let started = events
            .iter()
            .filter(|event| matches!(event, RefreshEvent::Started { .. }))
            .count();
        assert_eq!(started, 2);
        assert!(events.iter().any(|event| matches!(
            event,
            RefreshEvent::Finished(RefreshOutcome {
                reason: Some(RefreshReason::Periodic),
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
        // 闸门式 provider：refresh 卡到测试放行（这里始终不放行）。shutdown 只要
        // 能结束，就证明它没等 provider 完成；不需要拿 elapsed 上限去和模拟延迟
        // 赛跑——测试线程被抢占超过上限时，elapsed 断言会误报。
        let (gate_tx, gate_rx) = smol::channel::bounded(1);
        let mut manager = ProviderManager::new();
        manager.register(Arc::new(GatedProvider::new(
            "test:shutdown",
            Some(gate_rx),
            None,
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

        request_tx.send(RefreshRequest::Shutdown).await.unwrap();
        // provider 被闸门卡死时 shutdown 必须结束。5s 只是「不挂死」的兜底：
        // 真正卡住的 provider 永远不会完成，5s 内必返回即使真出错也不拖住套件。
        let finished = smol::future::or(
            async move {
                task.await;
                true
            },
            async {
                smol::Timer::after(Duration::from_secs(5)).await;
                false
            },
        )
        .await;
        assert!(finished, "shutdown waited for provider completion");

        // gate_tx 必须活到 task 结束：sender 一丢，闸门就变成 Closed。
        drop(gate_tx);
    });
}

#[test]
fn test_reload_providers_replaces_shared_manager_snapshot() {
    smol::block_on(async {
        let (event_tx, event_rx) = smol::channel::bounded(8);
        let manager = ProviderManagerHandle::new(ProviderManager::new());
        let initial = manager.snapshot();
        let coordinator = RefreshCoordinator::with_loader(
            manager.clone(),
            event_tx,
            Arc::new(ProviderManager::new),
        );
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

#[test]
fn slow_registry_reload_does_not_block_config_or_shutdown() {
    smol::block_on(async {
        let (entered_tx, entered_rx) = smol::channel::bounded(1);
        let (release_tx, release_rx) = smol::channel::bounded::<()>(1);
        let (event_tx, event_rx) = smol::channel::unbounded();
        let manager = ProviderManagerHandle::new(ProviderManager::new());
        let coordinator = RefreshCoordinator::with_loader(
            manager,
            event_tx,
            Arc::new(move || {
                entered_tx.try_send(()).unwrap();
                let _ = smol::block_on(release_rx.recv());
                ProviderManager::new()
            }),
        );
        let request_tx = coordinator.sender();
        let task = smol::spawn(coordinator.run());
        request_tx
            .send(RefreshRequest::ReloadProviders)
            .await
            .unwrap();
        entered_rx.recv().await.unwrap();
        request_tx
            .send(RefreshRequest::UpdateConfig {
                interval_mins: 9,
                enabled: Vec::new(),
                provider_credentials: ProviderSettings::default(),
            })
            .await
            .unwrap();
        request_tx
            .send(RefreshRequest::RefreshOne {
                id: ProviderId::BuiltIn(ProviderKind::Claude),
                reason: RefreshReason::Manual,
            })
            .await
            .unwrap();
        let event = smol::future::or(async { event_rx.recv().await.ok() }, async {
            smol::Timer::after(Duration::from_secs(1)).await;
            None
        })
        .await;
        assert!(matches!(
            event,
            Some(RefreshEvent::Finished(RefreshOutcome {
                result: RefreshResult::SkippedDisabled,
                ..
            }))
        ));
        request_tx.send(RefreshRequest::Shutdown).await.unwrap();
        let stopped = smol::future::or(
            async {
                task.await;
                true
            },
            async {
                smol::Timer::after(Duration::from_secs(1)).await;
                false
            },
        )
        .await;
        release_tx.try_send(()).unwrap();
        assert!(stopped);
    });
}

#[test]
fn repeated_registry_reloads_are_serial_and_only_publish_latest() {
    smol::block_on(async {
        let calls = Arc::new(AtomicUsize::new(0));
        let load_calls = calls.clone();
        let (entered_tx, entered_rx) = smol::channel::unbounded();
        let (release_tx, release_rx) = smol::channel::unbounded::<()>();
        let (event_tx, event_rx) = smol::channel::unbounded();
        let manager = ProviderManagerHandle::new(ProviderManager::new());
        let initial = manager.snapshot();
        let mut coordinator = RefreshCoordinator::with_loader(
            manager.clone(),
            event_tx,
            Arc::new(move || {
                let call = load_calls.fetch_add(1, Ordering::SeqCst);
                entered_tx.try_send(call).unwrap();
                let _ = smol::block_on(release_rx.recv());
                ProviderManager::new()
            }),
        );
        coordinator
            .handle_request(RefreshRequest::ReloadProviders)
            .await;
        assert_eq!(entered_rx.recv().await.unwrap(), 0);
        coordinator
            .handle_request(RefreshRequest::ReloadProviders)
            .await;
        coordinator
            .handle_request(RefreshRequest::ReloadProviders)
            .await;
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        release_tx.try_send(()).unwrap();
        let completed = coordinator.task_rx.recv().await.unwrap();
        coordinator.handle_task_message(completed).await;
        assert!(Arc::ptr_eq(&initial, &manager.snapshot()));
        assert!(event_rx.try_recv().is_err());
        assert_eq!(entered_rx.recv().await.unwrap(), 1);
        coordinator
            .handle_request(RefreshRequest::UpdateConfig {
                interval_mins: 11,
                enabled: Vec::new(),
                provider_credentials: ProviderSettings::default(),
            })
            .await;
        release_tx.try_send(()).unwrap();
        let completed = coordinator.task_rx.recv().await.unwrap();
        coordinator.handle_task_message(completed).await;
        assert!(!Arc::ptr_eq(&initial, &manager.snapshot()));
        assert_eq!(coordinator.scheduler.interval_mins(), 11);
        assert!(matches!(
            event_rx.try_recv(),
            Ok(RefreshEvent::ProvidersReloaded { .. })
        ));
        assert!(event_rx.try_recv().is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert!(coordinator.active_reload.is_none());
    });
}

#[test]
fn panicking_registry_reload_can_retry() {
    smol::block_on(async {
        let calls = Arc::new(AtomicUsize::new(0));
        let load_calls = calls.clone();
        let (event_tx, event_rx) = smol::channel::unbounded();
        let manager = ProviderManagerHandle::new(ProviderManager::new());
        let mut coordinator = RefreshCoordinator::with_loader(
            manager.clone(),
            event_tx,
            Arc::new(move || {
                if load_calls.fetch_add(1, Ordering::SeqCst) == 0 {
                    panic!("test loader panic");
                }
                ProviderManager::new()
            }),
        );

        coordinator
            .handle_request(RefreshRequest::ReloadProviders)
            .await;
        let failed = coordinator.task_rx.recv().await.unwrap();
        coordinator.handle_task_message(failed).await;
        assert!(coordinator.active_reload.is_none());
        assert!(event_rx.try_recv().is_err());

        coordinator
            .handle_request(RefreshRequest::ReloadProviders)
            .await;
        let completed = coordinator.task_rx.recv().await.unwrap();
        coordinator.handle_task_message(completed).await;

        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert!(coordinator.active_reload.is_none());
        assert!(matches!(
            event_rx.try_recv(),
            Ok(RefreshEvent::ProvidersReloaded { .. })
        ));
    });
}

#[test]
fn failed_registry_reload_does_not_drop_newer_request() {
    smol::block_on(async {
        let calls = Arc::new(AtomicUsize::new(0));
        let load_calls = calls.clone();
        let (entered_tx, entered_rx) = smol::channel::unbounded();
        let (release_tx, release_rx) = smol::channel::unbounded::<()>();
        let (event_tx, event_rx) = smol::channel::unbounded();
        let manager = ProviderManagerHandle::new(ProviderManager::new());
        let mut coordinator = RefreshCoordinator::with_loader(
            manager,
            event_tx,
            Arc::new(move || {
                let call = load_calls.fetch_add(1, Ordering::SeqCst);
                entered_tx.try_send(call).unwrap();
                if call == 0 {
                    let _ = smol::block_on(release_rx.recv());
                    panic!("test loader panic");
                }
                ProviderManager::new()
            }),
        );

        coordinator
            .handle_request(RefreshRequest::ReloadProviders)
            .await;
        assert_eq!(entered_rx.recv().await.unwrap(), 0);
        coordinator
            .handle_request(RefreshRequest::ReloadProviders)
            .await;
        release_tx.try_send(()).unwrap();

        let failed = coordinator.task_rx.recv().await.unwrap();
        coordinator.handle_task_message(failed).await;
        assert_eq!(entered_rx.recv().await.unwrap(), 1);

        let completed = coordinator.task_rx.recv().await.unwrap();
        coordinator.handle_task_message(completed).await;
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert!(coordinator.active_reload.is_none());
        assert!(matches!(
            event_rx.try_recv(),
            Ok(RefreshEvent::ProvidersReloaded { .. })
        ));
    });
}
