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
        if let Some(daily_pct) = usage.daily_remaining_percent {
            let used = 100.0 - daily_pct;
            let reset_text = usage
                .daily_reset_at_unix
                .and_then(|ts| {
                    chrono::DateTime::from_timestamp(ts, 0)
                        .map(|dt| dt.format("%Y-%m-%dT%H:%M:%SZ").to_string())
                })
                .as_deref()
                .and_then(time_utils::format_reset_countdown);

            quotas.push(QuotaInfo::with_details(
                "Daily Quota",
                used,
                100.0,
                QuotaType::ModelSpecific("Daily Quota".to_string()),
                reset_text,
            ));
        }

        if let Some(weekly_pct) = usage.weekly_remaining_percent {
            let used = 100.0 - weekly_pct;
            let reset_text = usage
                .weekly_reset_at_unix
                .and_then(|ts| {
                    chrono::DateTime::from_timestamp(ts, 0)
                        .map(|dt| dt.format("%Y-%m-%dT%H:%M:%SZ").to_string())
                })
                .as_deref()
                .and_then(time_utils::format_reset_countdown);

            quotas.push(QuotaInfo::with_details(
                "Weekly Quota",
                used,
                100.0,
                QuotaType::ModelSpecific("Weekly Quota".to_string()),
                reset_text,
            ));
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
    fn test_read_via_cached_plan_info() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO ItemTable (key, value) VALUES (?1, ?2)",
            [
                "windsurf.settings.cachedPlanInfo",
                r#"{"planName":"Pro","quotaUsage":{"dailyRemainingPercent":41,"weeklyRemainingPercent":70,"dailyResetAtUnix":1775462400,"weeklyResetAtUnix":1775980800}}"#,
            ],
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
