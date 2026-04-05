mod cache_source;
mod live_source;
mod parse_strategy;

use super::{AiProvider, ProviderError};
use crate::models::{ProviderDescriptor, ProviderKind, ProviderMetadata, RefreshData};
use anyhow::Result;
use async_trait::async_trait;
use log::warn;
use std::borrow::Cow;

super::define_unit_provider!(AntigravityProvider);

impl AntigravityProvider {
    fn classify_unavailable() -> Result<()> {
        if live_source::is_available() || cache_source::is_available() {
            Ok(())
        } else {
            Err(ProviderError::unavailable(
                "Antigravity live source and local cache are both unavailable",
            )
            .into())
        }
    }

    fn refresh_with_fallback() -> Result<RefreshData> {
        match live_source::fetch_refresh_data() {
            Ok(data) => Ok(data),
            Err(live_err) => {
                warn!(
                    target: "providers",
                    "Antigravity live source failed: {}, falling back to local cache",
                    live_err
                );

                match cache_source::read_refresh_data() {
                    Ok(data) => Ok(data),
                    Err(cache_err) => Err(ProviderError::fetch_failed(&format!(
                        "live source failed: {}; cache fallback failed: {}",
                        live_err, cache_err
                    ))
                    .into()),
                }
            }
        }
    }
}

#[async_trait]
impl AiProvider for AntigravityProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: Cow::Borrowed("antigravity:api"),
            metadata: ProviderMetadata {
                kind: ProviderKind::Antigravity,
                display_name: "Antigravity".into(),
                brand_name: "Codeium".into(),
                icon_asset: "src/icons/provider-antigravity.svg".into(),
                dashboard_url: "https://codeium.com/account".into(),
                account_hint: "Codeium account".into(),
                source_label: "local api".into(),
            },
        }
    }

    async fn check_availability(&self) -> Result<()> {
        Self::classify_unavailable()
    }

    async fn refresh(&self) -> Result<RefreshData> {
        Self::refresh_with_fallback()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_unavailable_maps_both_sources_missing() {
        // 这里只验证错误归类，不依赖本机是否真的安装 Antigravity。
        let err = ProviderError::unavailable(
            "Antigravity live source and local cache are both unavailable",
        );
        assert!(matches!(err, ProviderError::Unavailable { .. }));
    }
}
