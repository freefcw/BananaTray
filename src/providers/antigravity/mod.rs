mod cloud_source;

use super::codeium_family::{self, ANTIGRAVITY_SPEC};
use super::ProviderError;
use super::{AiProvider, ProviderCapabilities, ProviderExecutionContext, ProviderResult};
use crate::models::RefreshData;
use anyhow::Result;
use async_trait::async_trait;
use log::warn;

super::define_unit_provider!(AntigravityProvider);

#[async_trait]
impl AiProvider for AntigravityProvider {
    fn descriptor(&self) -> crate::models::ProviderDescriptor {
        codeium_family::descriptor(&ANTIGRAVITY_SPEC)
    }

    async fn check_availability(&self, _ctx: &ProviderExecutionContext<'_>) -> ProviderResult<()> {
        if cloud_source_is_configured()
            || codeium_family::classify_unavailable(&ANTIGRAVITY_SPEC).is_ok()
        {
            Ok(())
        } else {
            Err(ProviderError::unavailable(
                ANTIGRAVITY_SPEC.unavailable_message,
            ))
        }
    }

    async fn refresh(&self, _ctx: &ProviderExecutionContext<'_>) -> ProviderResult<RefreshData> {
        Ok(refresh_antigravity()?)
    }
}

/// 云端源可用性只看 Keychain token 是否存在（macOS）；
/// 不在此处做过期检查，把结构化的 SessionExpired 留给 refresh 阶段。
#[cfg(target_os = "macos")]
fn cloud_source_is_configured() -> bool {
    cloud_source::has_keychain_token()
}

#[cfg(not(target_os = "macos"))]
fn cloud_source_is_configured() -> bool {
    false
}

fn refresh_antigravity() -> Result<RefreshData> {
    refresh_antigravity_with_sources(
        cloud_source::fetch_refresh_data,
        || Ok(codeium_family::refresh_live(&ANTIGRAVITY_SPEC)?),
        || Ok(codeium_family::refresh_cache(&ANTIGRAVITY_SPEC)?),
    )
}

/// cloud → live → cache 编排；闭包注入便于单测 fallback 顺序。
fn refresh_antigravity_with_sources(
    fetch_cloud: impl FnOnce() -> Result<RefreshData>,
    fetch_live: impl FnOnce() -> Result<RefreshData>,
    fetch_cache: impl FnOnce() -> Result<RefreshData>,
) -> Result<RefreshData> {
    let cloud_err = match fetch_cloud() {
        Ok(data) => return Ok(data),
        Err(err) => err,
    };

    // token 过期是确定性失败，云端源无法自愈，但仍继续尝试本地源：
    // IDE 在跑时 live / cache 仍能给出真实数据。
    warn!(
        target: "providers",
        "{} cloud API failed: {}, trying local API",
        ANTIGRAVITY_SPEC.log_label,
        cloud_err
    );

    let live_err = match fetch_live() {
        Ok(data) => return Ok(data),
        Err(err) => err,
    };

    warn!(
        target: "providers",
        "{} local API failed: {}, falling back to local cache",
        ANTIGRAVITY_SPEC.log_label,
        live_err
    );

    fetch_cache().map_err(|cache_err| {
        ProviderError::fetch_failed(&format!(
            "all sources failed: cloud API error: {}; local API error: {}; cache error: {}",
            cloud_err, live_err, cache_err
        ))
        .into()
    })
}

impl ProviderCapabilities for AntigravityProvider {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{QuotaInfo, QuotaType};
    #[test]
    fn test_classify_unavailable_maps_both_sources_missing() {
        let err = ProviderError::unavailable(ANTIGRAVITY_SPEC.unavailable_message);
        assert!(matches!(err, ProviderError::Unavailable { .. }));
    }

    #[test]
    fn test_matches_antigravity_process_with_app_data_dir() {
        let line = "53319 /Applications/Antigravity.app/Contents/Resources/app/extensions/antigravity/bin/language_server_macos_arm --enable_lsp --csrf_token abc --extension_server_port 57048 --app_data_dir antigravity";
        assert!(codeium_family::matches_process_line(
            line,
            &ANTIGRAVITY_SPEC
        ));
    }

    #[test]
    fn test_matches_antigravity_process_with_path() {
        let line = "53319 /Applications/Antigravity.app/Contents/Resources/app/extensions/antigravity/bin/language_server_macos_arm --enable_lsp --csrf_token abc";
        assert!(codeium_family::matches_process_line(
            line,
            &ANTIGRAVITY_SPEC
        ));
    }

    #[test]
    fn test_matches_antigravity_linux_process_with_path() {
        let line = "53319 /usr/share/antigravity/resources/app/extensions/antigravity/bin/language_server_linux_x64 --enable_lsp";
        assert!(codeium_family::matches_process_line(
            line,
            &ANTIGRAVITY_SPEC
        ));
    }

    #[test]
    fn test_is_antigravity_process_rejects_devin_desktop() {
        let line = "10733 /Applications/Devin.app/Contents/Resources/app/extensions/windsurf/bin/language_server_macos_arm --api_server_url https://server.self-serve.windsurf.com --run_child --enable_lsp --extension_server_port 62503 --ide_name windsurf";
        assert!(!codeium_family::matches_process_line(
            line,
            &ANTIGRAVITY_SPEC
        ));
    }

    #[test]
    fn test_is_antigravity_process_with_dot_antigravity_path() {
        let line = "12345 /Users/test/.antigravity/bin/language_server_macos_arm --csrf_token abc";
        assert!(codeium_family::matches_process_line(
            line,
            &ANTIGRAVITY_SPEC
        ));
    }

    fn cloud_quota() -> RefreshData {
        RefreshData::with_account(
            vec![QuotaInfo::with_key_from_remaining_fraction(
                "cloud-daily",
                "Cloud Daily",
                0.8,
                QuotaType::General,
                None,
            )],
            None,
            None,
        )
        .with_source_label(cloud_source::CLOUD_API_SOURCE_LABEL)
    }

    fn live_quota() -> RefreshData {
        RefreshData::with_account(
            vec![QuotaInfo::with_key(
                "live-daily",
                "Daily",
                40.0,
                100.0,
                QuotaType::General,
                None,
            )],
            None,
            None,
        )
        .with_source_label("local api")
    }

    #[test]
    fn test_refresh_prefers_cloud_over_live() {
        let data = refresh_antigravity_with_sources(
            || Ok(cloud_quota()),
            || Ok(live_quota()),
            || -> Result<RefreshData> { panic!("cache should not run after cloud success") },
        )
        .unwrap();

        assert_eq!(
            data.source_label,
            Some(cloud_source::CLOUD_API_SOURCE_LABEL.to_string())
        );
        assert!(data.quotas.iter().any(|q| q.stable_key == "cloud-daily"));
    }

    #[test]
    fn test_refresh_falls_back_to_live_when_cloud_fails() {
        let data = refresh_antigravity_with_sources(
            || Err(anyhow::anyhow!("cloud unavailable")),
            || Ok(live_quota()),
            || -> Result<RefreshData> { panic!("cache should not run after live success") },
        )
        .unwrap();

        assert_eq!(data.source_label, Some("local api".to_string()));
        assert!(data.quotas.iter().any(|q| q.stable_key == "live-daily"));
    }

    #[test]
    fn test_refresh_falls_back_to_cache_when_cloud_and_live_fail() {
        let data = refresh_antigravity_with_sources(
            || Err(anyhow::anyhow!("cloud unavailable")),
            || Err(anyhow::anyhow!("live unavailable")),
            || Ok(live_quota().with_source_label("local cache")),
        )
        .unwrap();

        assert_eq!(data.source_label, Some("local cache".to_string()));
    }

    #[test]
    fn test_refresh_all_sources_failed_returns_merged_error() {
        let err = refresh_antigravity_with_sources(
            || Err(anyhow::anyhow!("cloud down")),
            || Err(anyhow::anyhow!("live down")),
            || Err(anyhow::anyhow!("cache down")),
        )
        .unwrap_err();

        let message = err.to_string();
        assert!(message.contains("cloud down"));
        assert!(message.contains("live down"));
        assert!(message.contains("cache down"));
    }

    #[test]
    fn test_session_expired_from_cloud_still_tries_local_sources() {
        // token 过期时云端源报 SessionExpired，但本地源仍可能给出真实数据
        let data = refresh_antigravity_with_sources(
            || Err(anyhow::Error::new(ProviderError::session_expired(None))),
            || Ok(live_quota()),
            || -> Result<RefreshData> { panic!("cache should not run after live success") },
        )
        .unwrap();

        assert_eq!(data.source_label, Some("local api".to_string()));
    }
}
