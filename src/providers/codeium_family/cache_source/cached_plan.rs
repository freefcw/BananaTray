use super::super::spec::CodeiumFamilySpec;
use super::super::LOCAL_CACHE_SOURCE_LABEL;
use super::auth_status::extract_email_from_auth_status;
use super::sqlite_store::query_cached_plan_info;
use crate::models::{QuotaDetailSpec, QuotaInfo, QuotaLabelSpec, QuotaType, RefreshData};
use crate::providers::{ProviderError, ProviderResult};
use log::{debug, warn};
use rusqlite::Connection;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CachedPlanInfo {
    pub(super) plan_name: Option<String>,
    #[serde(default)]
    pub(super) quota_usage: Option<QuotaUsageInfo>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct QuotaUsageInfo {
    pub(super) daily_remaining_percent: Option<f64>,
    pub(super) weekly_remaining_percent: Option<f64>,
    pub(super) daily_reset_at_unix: Option<i64>,
    pub(super) weekly_reset_at_unix: Option<i64>,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum CachedQuotaKind {
    Daily,
    Weekly,
}

impl CachedQuotaKind {
    fn stable_key(self) -> &'static str {
        match self {
            Self::Daily => "daily-quota",
            Self::Weekly => "weekly-quota",
        }
    }

    fn quota_type(self) -> QuotaType {
        match self {
            Self::Daily => QuotaType::General,
            Self::Weekly => QuotaType::Weekly,
        }
    }

    fn label_spec(self) -> QuotaLabelSpec {
        match self {
            Self::Daily => QuotaLabelSpec::Daily,
            Self::Weekly => QuotaLabelSpec::Weekly,
        }
    }
}

pub(super) fn read_via_cached_plan_info(
    conn: &Connection,
    spec: &CodeiumFamilySpec,
) -> ProviderResult<RefreshData> {
    let json_str = query_cached_plan_info(conn, spec)?;
    let plan_info = parse_cached_plan_info(&json_str)?;

    debug!(
        target: "providers",
        "{} cachedPlanInfo parsed: plan_name={:?}, quota_usage={:?}",
        spec.log_label,
        plan_info.plan_name,
        plan_info.quota_usage.is_some()
    );

    let plan_name = plan_info.plan_name;
    let mut quotas = Vec::new();

    if let Some(usage) = &plan_info.quota_usage {
        debug!(
            target: "providers",
            "{} quotaUsage: daily_remaining={:?}, weekly_remaining={:?}, daily_reset={:?}, weekly_reset={:?}",
            spec.log_label,
            usage.daily_remaining_percent,
            usage.weekly_remaining_percent,
            usage.daily_reset_at_unix,
            usage.weekly_reset_at_unix
        );

        if let Some(q) = build_quota_from_cached(
            CachedQuotaKind::Daily,
            usage.daily_remaining_percent,
            usage.daily_reset_at_unix,
            spec.log_label,
        ) {
            quotas.push(q);
        }

        if let Some(q) = build_quota_from_cached(
            CachedQuotaKind::Weekly,
            usage.weekly_remaining_percent,
            usage.weekly_reset_at_unix,
            spec.log_label,
        ) {
            quotas.push(q);
        } else if let (None, Some(reset_ts)) =
            (usage.weekly_remaining_percent, usage.weekly_reset_at_unix)
        {
            // 新版 Windsurf：weekly_remaining_percent 可能为 null（限额已满时）
            // 此时从 reset 时间推断配额状态：只要 reset 时间未过期，视为已满（0% remaining）
            let now_ts = chrono::Utc::now().timestamp();
            if reset_ts > now_ts {
                warn!(
                    target: "providers",
                    "{} weekly_remaining_percent is null but reset time is future; inferring 100% used (quota full)",
                    spec.log_label
                );
                quotas.push(QuotaInfo::with_key_from_remaining_percent(
                    CachedQuotaKind::Weekly.stable_key(),
                    CachedQuotaKind::Weekly.label_spec(),
                    0.0,
                    CachedQuotaKind::Weekly.quota_type(),
                    Some(QuotaDetailSpec::ResetAt {
                        epoch_secs: reset_ts,
                    }),
                ));
            }
        }
    }

    if quotas.is_empty() {
        return Err(ProviderError::no_data());
    }

    // cachedPlanInfo 中没有 email，从 auth status 中单独提取
    let email = extract_email_from_auth_status(conn, spec);

    Ok(RefreshData::with_account(quotas, email, plan_name)
        .with_source_label(LOCAL_CACHE_SOURCE_LABEL))
}

/// 根据 cachedPlanInfo 的 remaining_percent 和 reset_at_unix 构建配额项。
///
/// reset 时间已过时，缓存没有新周期的实际用量，不能把未知状态显示为满额。
///
/// `log_label` 用于在额度因 reset 过期而被丢弃时输出 debug 日志，让刷新日志
/// 能自洽地解释"解析出了 remaining_percent 却没有产生 quota"的原因。
pub(super) fn build_quota_from_cached(
    kind: CachedQuotaKind,
    remaining_percent: Option<f64>,
    reset_at_unix: Option<i64>,
    log_label: &str,
) -> Option<QuotaInfo> {
    let pct = remaining_percent?;

    if let Some(ts) = reset_at_unix {
        if ts <= chrono::Utc::now().timestamp() {
            debug!(
                target: "providers",
                "{} {} discarded: reset_at_unix={} is in the past (cache snapshot expired for this quota period)",
                log_label,
                kind.stable_key(),
                ts
            );
            return None;
        }
    }

    let reset_detail = reset_at_unix.map(|epoch_secs| QuotaDetailSpec::ResetAt { epoch_secs });

    Some(QuotaInfo::with_key_from_remaining_percent(
        kind.stable_key(),
        kind.label_spec(),
        pct,
        kind.quota_type(),
        reset_detail,
    ))
}

pub(super) fn parse_cached_plan_info(json_str: &str) -> ProviderResult<CachedPlanInfo> {
    serde_json::from_str(json_str)
        .map_err(|e| ProviderError::parse_failed(&format!("invalid cachedPlanInfo JSON: {}", e)))
}
