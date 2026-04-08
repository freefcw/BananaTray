use super::parse_strategy::{CacheParseStrategy, ParseStrategy};
use super::spec::CodeiumFamilySpec;
use crate::models::{QuotaInfo, QuotaType, RefreshData};
use crate::providers::ProviderError;
use crate::utils::time_utils;
use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use log::{debug, warn};
use rusqlite::{Connection, OpenFlags};
use serde::Deserialize;
use std::path::PathBuf;

pub fn is_available(spec: &CodeiumFamilySpec) -> bool {
    cache_db_path(spec).is_ok()
}

pub fn read_refresh_data(spec: &CodeiumFamilySpec) -> Result<RefreshData> {
    let db_path = cache_db_path(spec)?;
    let conn = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| {
            format!(
                "cannot open {} cache DB: {}",
                spec.log_label,
                db_path.display()
            )
        })?;

    // 策略 1: 传统 protobuf 解析
    let proto_result = read_via_protobuf(&conn, spec);
    if proto_result.is_ok() {
        return proto_result;
    }
    let proto_err = proto_result.unwrap_err();

    // 策略 2: cachedPlanInfo JSON 回退（新版 Windsurf）
    if !spec.cached_plan_info_key_candidates.is_empty() {
        warn!(
            target: "providers",
            "{} protobuf decode failed: {}, trying cachedPlanInfo fallback",
            spec.log_label,
            proto_err
        );
        match read_via_cached_plan_info(&conn, spec) {
            Ok(data) => return Ok(data),
            Err(plan_err) => {
                warn!(
                    target: "providers",
                    "{} cachedPlanInfo fallback also failed: {}",
                    spec.log_label,
                    plan_err
                );
            }
        }
    }

    Err(proto_err)
}

fn read_via_protobuf(conn: &Connection, spec: &CodeiumFamilySpec) -> Result<RefreshData> {
    let auth_status_json = query_auth_status_json(conn, spec)?;
    let user_status_data = decode_user_status_payload(&auth_status_json)?;
    let strategy = CacheParseStrategy;
    let (quotas, email, plan_name) = strategy.parse(&user_status_data)?;
    Ok(RefreshData::with_account(quotas, email, plan_name))
}

fn cache_db_path(spec: &CodeiumFamilySpec) -> Result<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| ProviderError::unavailable("cannot determine home directory"))?;
    let db_path = home.join(spec.cache_db_relative_path);

    if !db_path.exists() {
        return Err(ProviderError::unavailable(&format!(
            "{} local cache database not found",
            spec.log_label
        ))
        .into());
    }

    debug!(
        target: "providers",
        "{} local cache DB: {}",
        spec.log_label,
        db_path.display()
    );
    Ok(db_path)
}

pub(super) fn query_auth_status_json(
    conn: &Connection,
    spec: &CodeiumFamilySpec,
) -> Result<String> {
    for key in spec.auth_status_key_candidates {
        match conn.query_row("SELECT value FROM ItemTable WHERE key = ?1", [key], |row| {
            row.get(0)
        }) {
            Ok(value) => return Ok(value),
            Err(rusqlite::Error::QueryReturnedNoRows) => continue,
            Err(e) => {
                return Err(
                    ProviderError::parse_failed(&format!("cannot query {}: {}", key, e)).into(),
                )
            }
        }
    }

    Err(ProviderError::parse_failed(&format!(
        "cannot find auth status key in local cache: {}",
        spec.auth_status_key_candidates.join(", ")
    ))
    .into())
}

fn decode_user_status_payload(auth_status_json: &str) -> Result<Vec<u8>> {
    let auth_status: serde_json::Value = serde_json::from_str(auth_status_json)
        .map_err(|e| ProviderError::parse_failed(&format!("invalid auth status JSON: {}", e)))?;

    let user_status_b64 = auth_status
        .get("userStatusProtoBinaryBase64")
        .and_then(|value| value.as_str())
        .ok_or_else(|| ProviderError::parse_failed("missing userStatusProtoBinaryBase64 field"))?;

    STANDARD.decode(user_status_b64).map_err(|e| {
        ProviderError::parse_failed(&format!("invalid user status base64: {}", e)).into()
    })
}

// ---------------------------------------------------------------------------
// cachedPlanInfo JSON 回退策略（新版 Windsurf）
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CachedPlanInfo {
    plan_name: Option<String>,
    #[serde(default)]
    quota_usage: Option<QuotaUsageInfo>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuotaUsageInfo {
    daily_remaining_percent: Option<f64>,
    weekly_remaining_percent: Option<f64>,
    daily_reset_at_unix: Option<i64>,
    weekly_reset_at_unix: Option<i64>,
}

fn read_via_cached_plan_info(conn: &Connection, spec: &CodeiumFamilySpec) -> Result<RefreshData> {
    let json_str = query_cached_plan_info(conn, spec)?;
    let plan_info = parse_cached_plan_info(&json_str)?;

    let plan_name = plan_info.plan_name;
    let mut quotas = Vec::new();

    if let Some(usage) = plan_info.quota_usage {
        if let Some(q) = build_quota_from_cached(
            "Daily Quota",
            usage.daily_remaining_percent,
            usage.daily_reset_at_unix,
        ) {
            quotas.push(q);
        }

        if let Some(q) = build_quota_from_cached(
            "Weekly Quota",
            usage.weekly_remaining_percent,
            usage.weekly_reset_at_unix,
        ) {
            quotas.push(q);
        }
    }

    if quotas.is_empty() {
        anyhow::bail!("no quota data found in cachedPlanInfo");
    }

    // cachedPlanInfo 中没有 email，但可以从 auth_status_json 中尝试提取
    let email = query_auth_status_json(conn, spec)
        .ok()
        .and_then(|json_str| serde_json::from_str::<serde_json::Value>(&json_str).ok())
        .and_then(|v| v.get("email")?.as_str().map(|s| s.to_string()))
        .filter(|s| !s.is_empty());

    Ok(RefreshData::with_account(quotas, email, plan_name))
}

/// 根据 cachedPlanInfo 的 remaining_percent 和 reset_at_unix 构建配额项。
///
/// 关键逻辑：如果 reset 时间已过期，说明配额已被重置，
/// 缓存中的 remaining_percent 是旧数据，应视为 100% remaining。
fn build_quota_from_cached(
    label: &str,
    remaining_percent: Option<f64>,
    reset_at_unix: Option<i64>,
) -> Option<QuotaInfo> {
    let pct = remaining_percent?;

    let now_ts = chrono::Utc::now().timestamp();
    let is_stale = reset_at_unix.is_some_and(|ts| ts <= now_ts);

    // 过期 → 配额已重置，视为全部可用
    let effective_remaining = if is_stale { 100.0 } else { pct };
    let used = 100.0 - effective_remaining;

    let reset_text = if is_stale {
        // 重置时间已过，不再展示倒计时
        None
    } else {
        reset_at_unix
            .and_then(|ts| {
                chrono::DateTime::from_timestamp(ts, 0)
                    .map(|dt| dt.format("%Y-%m-%dT%H:%M:%SZ").to_string())
            })
            .as_deref()
            .and_then(time_utils::format_reset_countdown)
    };

    Some(QuotaInfo::with_details(
        label,
        used,
        100.0,
        QuotaType::ModelSpecific(label.to_string()),
        reset_text,
    ))
}

fn query_cached_plan_info(conn: &Connection, spec: &CodeiumFamilySpec) -> Result<String> {
    for key in spec.cached_plan_info_key_candidates {
        match conn.query_row("SELECT value FROM ItemTable WHERE key = ?1", [key], |row| {
            row.get(0)
        }) {
            Ok(value) => {
                debug!(
                    target: "providers",
                    "{} found cachedPlanInfo via key '{}'",
                    spec.log_label,
                    key
                );
                return Ok(value);
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => continue,
            Err(e) => {
                return Err(
                    ProviderError::parse_failed(&format!("cannot query {}: {}", key, e)).into(),
                )
            }
        }
    }

    Err(ProviderError::parse_failed(&format!(
        "cannot find cachedPlanInfo key in local cache: {}",
        spec.cached_plan_info_key_candidates.join(", ")
    ))
    .into())
}

fn parse_cached_plan_info(json_str: &str) -> Result<CachedPlanInfo> {
    serde_json::from_str(json_str).map_err(|e| {
        ProviderError::parse_failed(&format!("invalid cachedPlanInfo JSON: {}", e)).into()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_windsurf_spec() -> CodeiumFamilySpec {
        CodeiumFamilySpec {
            kind: crate::models::ProviderKind::Windsurf,
            provider_id: "windsurf:api",
            display_name: "Windsurf",
            brand_name: "Codeium",
            icon_asset: "src/icons/provider-windsurf.svg",
            dashboard_url: "https://windsurf.com/",
            account_hint: "Windsurf account",
            source_label: "local api",
            log_label: "Windsurf",
            ide_name: "windsurf",
            unavailable_message: "Windsurf live source and local cache are both unavailable",
            cache_db_relative_path:
                "Library/Application Support/Windsurf/User/globalStorage/state.vscdb",
            auth_status_key_candidates: &["windsurfAuthStatus", "antigravityAuthStatus"],
            process_markers: &["--ide_name windsurf", "/windsurf/", "/windsurf.app/"],
            cached_plan_info_key_candidates: &["windsurf.settings.cachedPlanInfo"],
        }
    }

    #[test]
    fn test_decode_user_status_payload_success() {
        let payload = STANDARD.encode(b"proto-bytes");
        let json = format!(r#"{{"userStatusProtoBinaryBase64":"{}"}}"#, payload);

        let data = decode_user_status_payload(&json).unwrap();
        assert_eq!(data, b"proto-bytes");
    }

    #[test]
    fn test_decode_user_status_payload_missing_field() {
        let err = decode_user_status_payload(r#"{"other":"value"}"#).unwrap_err();
        let provider_err = ProviderError::classify(&err);
        assert!(matches!(provider_err, ProviderError::ParseFailed { .. }));
    }

    #[test]
    fn test_query_auth_status_json_uses_fallback_keys() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO ItemTable (key, value) VALUES (?1, ?2)",
            ["antigravityAuthStatus", "payload-json"],
        )
        .unwrap();

        let spec = test_windsurf_spec();
        let value = query_auth_status_json(&conn, &spec).unwrap();
        assert_eq!(value, "payload-json");
    }

    #[test]
    fn test_parse_cached_plan_info_full() {
        let json = r#"{
            "planName": "Pro",
            "quotaUsage": {
                "dailyRemainingPercent": 41,
                "weeklyRemainingPercent": 70,
                "dailyResetAtUnix": 1775462400,
                "weeklyResetAtUnix": 1775980800
            }
        }"#;

        let info = parse_cached_plan_info(json).unwrap();
        assert_eq!(info.plan_name, Some("Pro".to_string()));
        let usage = info.quota_usage.unwrap();
        assert_eq!(usage.daily_remaining_percent, Some(41.0));
        assert_eq!(usage.weekly_remaining_percent, Some(70.0));
        assert_eq!(usage.daily_reset_at_unix, Some(1775462400));
    }

    #[test]
    fn test_parse_cached_plan_info_minimal() {
        let json = r#"{"planName": "Free"}"#;
        let info = parse_cached_plan_info(json).unwrap();
        assert_eq!(info.plan_name, Some("Free".to_string()));
        assert!(info.quota_usage.is_none());
    }

    #[test]
    fn test_read_via_cached_plan_info_fresh() {
        // 使用未来的 reset 时间，数据不过期
        let future_daily = chrono::Utc::now().timestamp() + 3600; // 1 小时后
        let future_weekly = chrono::Utc::now().timestamp() + 86400 * 5; // 5 天后
        let json_value = format!(
            r#"{{"planName":"Pro","quotaUsage":{{"dailyRemainingPercent":41,"weeklyRemainingPercent":70,"dailyResetAtUnix":{},"weeklyResetAtUnix":{}}}}}"#,
            future_daily, future_weekly
        );

        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO ItemTable (key, value) VALUES (?1, ?2)",
            ["windsurf.settings.cachedPlanInfo", &json_value],
        )
        .unwrap();

        let spec = test_windsurf_spec();
        let data = read_via_cached_plan_info(&conn, &spec).unwrap();
        assert_eq!(data.account_tier, Some("Pro".to_string()));
        assert_eq!(data.quotas.len(), 2);
        assert_eq!(data.quotas[0].label, "Daily Quota");
        assert!((data.quotas[0].used - 59.0).abs() < 0.01); // 100 - 41 = 59
        assert_eq!(data.quotas[1].label, "Weekly Quota");
        assert!((data.quotas[1].used - 30.0).abs() < 0.01); // 100 - 70 = 30
    }

    #[test]
    fn test_build_quota_from_cached_fresh() {
        let future_ts = chrono::Utc::now().timestamp() + 3600;
        let q = build_quota_from_cached("Daily Quota", Some(41.0), Some(future_ts)).unwrap();
        assert_eq!(q.label, "Daily Quota");
        assert!((q.used - 59.0).abs() < 0.01);
        assert!(q.detail_text.is_some()); // 有倒计时
    }

    #[test]
    fn test_build_quota_from_cached_stale_resets_to_full() {
        // reset 时间已过期 → 配额已重置，应视为 0% used
        let past_ts = chrono::Utc::now().timestamp() - 3600;
        let q = build_quota_from_cached("Daily Quota", Some(41.0), Some(past_ts)).unwrap();
        assert_eq!(q.label, "Daily Quota");
        assert!((q.used - 0.0).abs() < 0.01); // 过期后重置为 0% used
        assert!(q.detail_text.is_none()); // 不展示过期的倒计时
    }

    #[test]
    fn test_build_quota_from_cached_no_reset_time() {
        // 没有 reset 时间 → 按原始数据展示
        let q = build_quota_from_cached("Weekly Quota", Some(70.0), None).unwrap();
        assert!((q.used - 30.0).abs() < 0.01);
    }

    #[test]
    fn test_build_quota_from_cached_no_percent() {
        // 没有百分比数据 → 返回 None
        let q = build_quota_from_cached("Daily Quota", None, Some(9999999999));
        assert!(q.is_none());
    }

    #[test]
    fn test_query_cached_plan_info_not_found() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
            [],
        )
        .unwrap();

        let spec = test_windsurf_spec();
        let err = query_cached_plan_info(&conn, &spec).unwrap_err();
        let provider_err = ProviderError::classify(&err);
        assert!(matches!(provider_err, ProviderError::ParseFailed { .. }));
    }
}
