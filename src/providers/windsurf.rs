mod seat_source;

use super::codeium_family::{self, WINDSURF_SPEC};
use super::ProviderError;
use super::{AiProvider, ProviderResult};
use crate::models::{QuotaType, RefreshData};
use anyhow::Result;
use async_trait::async_trait;
use log::{debug, warn};

super::define_unit_provider!(WindsurfProvider);

const SEAT_API_SOURCE_LABEL: &str = "seat api";
const SEAT_AND_CACHE_SOURCE_LABEL: &str = "seat api + local cache";

#[async_trait]
impl AiProvider for WindsurfProvider {
    fn descriptor(&self) -> crate::models::ProviderDescriptor {
        codeium_family::descriptor(&WINDSURF_SPEC)
    }

    async fn check_availability(&self) -> ProviderResult<()> {
        Ok(codeium_family::classify_unavailable(&WINDSURF_SPEC)?)
    }

    async fn refresh(&self) -> ProviderResult<RefreshData> {
        Ok(refresh_windsurf()?)
    }
}

fn refresh_windsurf() -> Result<RefreshData> {
    refresh_windsurf_with_sources(
        || seat_source::fetch_refresh_data(&WINDSURF_SPEC),
        || Ok(codeium_family::refresh_live(&WINDSURF_SPEC)?),
        || Ok(codeium_family::refresh_cache(&WINDSURF_SPEC)?),
    )
}

fn refresh_windsurf_with_sources(
    fetch_seat: impl FnOnce() -> Result<RefreshData>,
    fetch_live: impl FnOnce() -> Result<RefreshData>,
    fetch_cache: impl Fn() -> Result<RefreshData>,
) -> Result<RefreshData> {
    match fetch_seat() {
        Ok(seat_data) => {
            if seat_data.quotas.len() == 1 {
                match fetch_cache() {
                    Ok(cache_data) => {
                        return Ok(merge_seat_and_cache_quotas(&seat_data, &cache_data));
                    }
                    Err(cache_err) => {
                        debug!(
                            target: "providers",
                            "{} cache fallback for weekly quota failed: {}, returning seat data only",
                            WINDSURF_SPEC.log_label,
                            cache_err
                        );
                    }
                }
            }
            Ok(seat_data)
        }
        Err(seat_err) => {
            warn!(
                target: "providers",
                "{} seat management API failed: {}, trying local API",
                WINDSURF_SPEC.log_label,
                seat_err
            );

            match fetch_live() {
                Ok(data) => Ok(data),
                Err(live_err) => {
                    warn!(
                        target: "providers",
                        "{} local API failed: {}, falling back to local cache",
                        WINDSURF_SPEC.log_label,
                        live_err
                    );

                    match fetch_cache() {
                        Ok(data) => Ok(data),
                        Err(cache_err) => Err(ProviderError::fetch_failed(&format!(
                            "all sources failed: seat API error: {}; local API error: {}; cache error: {}",
                            seat_err, live_err, cache_err
                        ))
                        .into()),
                    }
                }
            }
        }
    }
}

/// 合并 Seat API 和 Cache 数据：用 Seat 的实时日配额 + Cache 的周配额。
fn merge_seat_and_cache_quotas(seat_data: &RefreshData, cache_data: &RefreshData) -> RefreshData {
    let mut merged_quotas = seat_data.quotas.clone();
    let mut cache_contributed = false;

    for quota in &cache_data.quotas {
        let is_weekly =
            quota.quota_type == QuotaType::Weekly || quota.stable_key.contains("weekly");
        if is_weekly
            && !merged_quotas
                .iter()
                .any(|q| q.stable_key == quota.stable_key)
        {
            merged_quotas.push(quota.clone());
            cache_contributed = true;
        }
    }

    let account_email = seat_data
        .account_email
        .clone()
        .or_else(|| cache_data.account_email.clone());
    let account_tier = seat_data
        .account_tier
        .clone()
        .or_else(|| cache_data.account_tier.clone());

    if seat_data.account_email.is_none() && account_email.is_some() {
        cache_contributed = true;
    }
    if seat_data.account_tier.is_none() && account_tier.is_some() {
        cache_contributed = true;
    }

    let merged = RefreshData::with_account(merged_quotas, account_email, account_tier);

    if cache_contributed {
        merged.with_source_label(SEAT_AND_CACHE_SOURCE_LABEL)
    } else {
        merged.with_source_label(
            seat_data
                .source_label
                .as_deref()
                .unwrap_or(SEAT_API_SOURCE_LABEL),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{QuotaInfo, QuotaLabelSpec, QuotaType};

    fn daily_quota(used: f64) -> QuotaInfo {
        QuotaInfo::with_key(
            "daily-quota",
            QuotaLabelSpec::Daily,
            used,
            100.0,
            QuotaType::General,
            None,
        )
    }

    fn weekly_quota(used: f64) -> QuotaInfo {
        QuotaInfo::with_key(
            "weekly-quota",
            QuotaLabelSpec::Weekly,
            used,
            100.0,
            QuotaType::Weekly,
            None,
        )
    }

    #[test]
    fn test_classify_unavailable_maps_both_sources_missing() {
        let err = ProviderError::unavailable(WINDSURF_SPEC.unavailable_message);
        assert!(matches!(err, ProviderError::Unavailable { .. }));
    }

    #[test]
    fn test_matches_windsurf_process() {
        let line = "3483 /Applications/Windsurf.app/Contents/Resources/app/extensions/windsurf/bin/language_server_macos_arm --api_server_url https://server.codeium.com --run_child --enable_lsp --csrf_token abc --extension_server_port 55114 --ide_name windsurf";
        assert!(codeium_family::matches_process_line(line, &WINDSURF_SPEC));
    }

    #[test]
    fn test_matches_windsurf_linux_process() {
        let line = "3483 /usr/share/windsurf/resources/app/extensions/windsurf/bin/language_server_linux_x64 --api_server_url https://server.codeium.com --run_child --enable_lsp --extension_server_port 55114 --ide_name windsurf";
        assert!(codeium_family::matches_process_line(line, &WINDSURF_SPEC));
    }

    #[test]
    fn test_rejects_antigravity_process() {
        let line = "53319 /Applications/Antigravity.app/Contents/Resources/app/extensions/antigravity/bin/language_server_macos_arm --enable_lsp --csrf_token abc --extension_server_port 57048 --app_data_dir antigravity";
        assert!(!codeium_family::matches_process_line(line, &WINDSURF_SPEC));
    }

    #[test]
    fn test_refresh_prefers_seat_api_over_stale_live_data() {
        let seat_data = RefreshData::with_account(
            vec![daily_quota(35.0), weekly_quota(17.0)],
            Some("seat@example.com".to_string()),
            Some("Pro".to_string()),
        )
        .with_source_label(SEAT_API_SOURCE_LABEL);

        let live_data = RefreshData::with_account(
            vec![daily_quota(12.0), weekly_quota(6.0)],
            Some("live@example.com".to_string()),
            Some("Pro".to_string()),
        )
        .with_source_label("local api");

        let data = refresh_windsurf_with_sources(
            || Ok(seat_data.clone()),
            || Ok(live_data.clone()),
            || Ok(RefreshData::with_account(vec![], None, None)),
        )
        .unwrap();

        assert_eq!(data.source_label, Some(SEAT_API_SOURCE_LABEL.to_string()));
        assert!(data
            .quotas
            .iter()
            .any(|q| q.stable_key == "daily-quota" && (q.used - 35.0).abs() < 0.01));
        assert!(data
            .quotas
            .iter()
            .any(|q| q.stable_key == "weekly-quota" && (q.used - 17.0).abs() < 0.01));
        assert_eq!(data.account_email, Some("seat@example.com".to_string()));
    }

    #[test]
    fn test_refresh_falls_back_to_live_when_seat_api_fails() {
        let live_data = RefreshData::with_account(
            vec![daily_quota(12.0), weekly_quota(6.0)],
            Some("live@example.com".to_string()),
            Some("Pro".to_string()),
        )
        .with_source_label("local api");

        let data = refresh_windsurf_with_sources(
            || Err(anyhow::anyhow!("seat unavailable")),
            || Ok(live_data.clone()),
            || Ok(RefreshData::with_account(vec![], None, None)),
        )
        .unwrap();

        assert_eq!(data.source_label, Some("local api".to_string()));
        assert!(data
            .quotas
            .iter()
            .any(|q| q.stable_key == "daily-quota" && (q.used - 12.0).abs() < 0.01));
    }

    #[test]
    fn test_merge_seat_and_cache_quotas_adds_weekly() {
        let seat_data = RefreshData::with_account(
            vec![daily_quota(50.0)],
            Some("test@example.com".to_string()),
            Some("Pro".to_string()),
        )
        .with_source_label(SEAT_API_SOURCE_LABEL);

        let cache_data = RefreshData::with_account(
            vec![daily_quota(45.0), weekly_quota(80.0)],
            Some("test@example.com".to_string()),
            Some("Pro".to_string()),
        )
        .with_source_label("local cache");

        let merged = merge_seat_and_cache_quotas(&seat_data, &cache_data);

        assert_eq!(merged.quotas.len(), 2);
        // 主路径：cache 周配额走 quota_type == Weekly 判定，被合并到结果
        assert!(merged
            .quotas
            .iter()
            .any(|q| q.stable_key == "weekly-quota" && q.quota_type == QuotaType::Weekly));
        // seat daily 保留，未被 cache daily 覆盖
        assert!(merged
            .quotas
            .iter()
            .any(|q| q.stable_key == "daily-quota" && (q.used - 50.0).abs() < 0.01));
        assert_eq!(merged.account_email, Some("test@example.com".to_string()));
        assert_eq!(merged.account_tier, Some("Pro".to_string()));
        assert_eq!(
            merged.source_label,
            Some(SEAT_AND_CACHE_SOURCE_LABEL.to_string())
        );
    }

    #[test]
    fn test_merge_seat_and_cache_quotas_falls_back_to_cache_account_metadata() {
        let seat_data = RefreshData::with_account(vec![daily_quota(20.0)], None, None)
            .with_source_label(SEAT_API_SOURCE_LABEL);

        let cache_data = RefreshData::with_account(
            vec![],
            Some("cached@example.com".to_string()),
            Some("Pro".to_string()),
        )
        .with_source_label("local cache");

        let merged = merge_seat_and_cache_quotas(&seat_data, &cache_data);

        assert_eq!(merged.account_email, Some("cached@example.com".to_string()));
        assert_eq!(merged.account_tier, Some("Pro".to_string()));
        assert_eq!(
            merged.source_label,
            Some(SEAT_AND_CACHE_SOURCE_LABEL.to_string())
        );
    }
}
