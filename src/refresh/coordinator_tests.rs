use super::*;
use crate::models::ErrorKind;
use crate::models::{ProviderDescriptor, ProviderId, ProviderKind, ProviderMetadata, RefreshData};
use crate::providers::error_presenter::ProviderErrorPresenter;
use crate::providers::{AiProvider, ProviderManager};
use anyhow::Result;
use async_trait::async_trait;
use std::borrow::Cow;
use std::sync::Arc;
use std::time::Duration;

// ============================================================================
// ProviderError 分类测试（build_outcome 使用的错误转换）
// ============================================================================

#[test]
fn test_classify_error_kind_config_missing() {
    let error = crate::providers::ProviderError::ConfigMissing {
        key: "github_token".to_string(),
    };
    assert_eq!(
        ProviderErrorPresenter::to_error_kind(&error),
        ErrorKind::ConfigMissing
    );
}

#[test]
fn test_classify_error_kind_auth_required() {
    let error = crate::providers::ProviderError::AuthRequired { hint: None };
    assert_eq!(
        ProviderErrorPresenter::to_error_kind(&error),
        ErrorKind::AuthRequired
    );
}

#[test]
fn test_classify_error_kind_session_expired() {
    let error = crate::providers::ProviderError::SessionExpired {
        hint: Some("re-login".to_string()),
    };
    assert_eq!(
        ProviderErrorPresenter::to_error_kind(&error),
        ErrorKind::AuthRequired
    );
}

#[test]
fn test_classify_error_kind_network_error() {
    let error = crate::providers::ProviderError::Timeout;
    assert_eq!(
        ProviderErrorPresenter::to_error_kind(&error),
        ErrorKind::NetworkError
    );

    let error = crate::providers::ProviderError::NetworkFailed {
        reason: "timeout".to_string(),
    };
    assert_eq!(
        ProviderErrorPresenter::to_error_kind(&error),
        ErrorKind::NetworkError
    );
}

#[test]
fn test_classify_error_kind_unknown() {
    let error = crate::providers::ProviderError::CliNotFound {
        cli_name: "claude".to_string(),
    };
    assert_eq!(
        ProviderErrorPresenter::to_error_kind(&error),
        ErrorKind::Unknown
    );

    let error = crate::providers::ProviderError::ParseFailed {
        reason: "invalid json".to_string(),
    };
    assert_eq!(
        ProviderErrorPresenter::to_error_kind(&error),
        ErrorKind::Unknown
    );
}

struct DelayedProvider {
    id: String,
    delay: Duration,
}

impl DelayedProvider {
    fn new(id: &str, delay: Duration) -> Self {
        Self {
            id: id.to_string(),
            delay,
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

    async fn refresh(&self) -> Result<RefreshData> {
        std::thread::sleep(self.delay);
        Ok(RefreshData::quotas_only(Vec::new()))
    }
}

#[test]
fn test_execute_refresh_concurrent_reports_completion_order() {
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
        let mut coordinator = RefreshCoordinator::new(Arc::new(manager), event_tx);
        coordinator
            .scheduler
            .update_config(10, vec![slow_id.clone(), fast_id.clone()]);

        coordinator
            .execute_refresh_concurrent(
                vec![slow_id.clone(), fast_id.clone()],
                RefreshReason::Manual,
            )
            .await;

        let mut finished_ids = Vec::new();
        for _ in 0..4 {
            match event_rx.recv().await.unwrap() {
                RefreshEvent::Finished(outcome) => finished_ids.push(outcome.id),
                RefreshEvent::Started { .. } => {}
                RefreshEvent::ProvidersReloaded { .. } => unreachable!(),
            }
        }

        assert_eq!(finished_ids, vec![fast_id, slow_id]);
    });
}
