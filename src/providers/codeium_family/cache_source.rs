use super::parse_strategy::{CacheParseStrategy, ParseStrategy};
use super::spec::CodeiumFamilySpec;
use super::LOCAL_CACHE_SOURCE_LABEL;
use crate::models::{QuotaDetailSpec, QuotaInfo, QuotaType, RefreshData};
use crate::providers::ProviderError;
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
    Ok(RefreshData::with_account(quotas, email, plan_name)
        .with_source_label(LOCAL_CACHE_SOURCE_LABEL))
}

pub(in crate::providers::codeium_family) fn cache_db_path(
    spec: &CodeiumFamilySpec,
) -> Result<PathBuf> {
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

pub(in crate::providers::codeium_family) fn query_auth_status_json(
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
                quotas.push(QuotaInfo::with_details(
                    "Weekly Quota",
                    100.0, // 已满：100% used
                    100.0,
                    QuotaType::ModelSpecific("Weekly Quota".to_string()),
                    Some(QuotaDetailSpec::ResetAt {
                        epoch_secs: reset_ts,
                    }),
                ));
            }
        }
    }

    if quotas.is_empty() {
        anyhow::bail!("no quota data found in cachedPlanInfo");
    }

    // cachedPlanInfo 中没有 email，从 auth status 中单独提取
    let email = extract_email_from_auth_status(conn, spec);

    Ok(RefreshData::with_account(quotas, email, plan_name)
        .with_source_label(LOCAL_CACHE_SOURCE_LABEL))
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

    let reset_detail = if is_stale {
        // 重置时间已过，不再展示倒计时
        None
    } else {
        reset_at_unix.map(|epoch_secs| QuotaDetailSpec::ResetAt { epoch_secs })
    };

    Some(QuotaInfo::with_details(
        label,
        used,
        100.0,
        QuotaType::ModelSpecific(label.to_string()),
        reset_detail,
    ))
}

/// UserStatus protobuf 中 email 字段的 field number（来自 Codeium API schema）
const PROTO_FIELD_EMAIL: u32 = 7;

/// 从 auth status JSON 中提取用户 email。
///
/// 支持两种格式：
/// 1. 旧格式：JSON 顶层有 `email` 字段
/// 2. 新格式（当前 Windsurf）：JSON 只有 `userStatusProtoBinaryBase64`，
///    但 protobuf 中含非法 wire type 导致 prost::decode 整体失败。
///    用宽容扫描在遇到非法字节前提取 email field。
fn extract_email_from_auth_status(conn: &Connection, spec: &CodeiumFamilySpec) -> Option<String> {
    let json_str = query_auth_status_json(conn, spec).ok()?;
    let v: serde_json::Value = serde_json::from_str(&json_str).ok()?;

    // 旧格式：顶层 email 字段
    if let Some(email) = v
        .get("email")
        .and_then(|e| e.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
    {
        return Some(email);
    }

    // 新格式：从 protobuf 二进制做宽容扫描
    v.get("userStatusProtoBinaryBase64")
        .and_then(|b| b.as_str())
        .and_then(|b64| STANDARD.decode(b64).ok())
        .and_then(|bytes| extract_string_field_permissive(&bytes, PROTO_FIELD_EMAIL))
        .filter(|s| !s.is_empty())
}

/// 宽容扫描 protobuf 字节，提取指定 field_number 的第一个 length-delimited（wire=2）string 字段。
/// 遇到非法 wire type 时停止而不报错，确保在截断数据中也能提取已出现的字段。
fn extract_string_field_permissive(data: &[u8], field_number: u32) -> Option<String> {
    let mut i = 0;
    while i < data.len() {
        let b = data[i];
        let wire = b & 0x7;
        let field = (b >> 3) as u32;
        i += 1;

        match wire {
            2 => {
                // length-delimited：读 varint 长度
                let mut length: usize = 0;
                let mut shift = 0usize;
                loop {
                    if i >= data.len() {
                        return None;
                    }
                    let b2 = data[i];
                    i += 1;
                    length |= ((b2 & 0x7f) as usize) << shift;
                    if b2 & 0x80 == 0 {
                        break;
                    }
                    shift += 7;
                }
                if i + length > data.len() {
                    return None;
                }
                let val = &data[i..i + length];
                i += length;
                if field == field_number {
                    return std::str::from_utf8(val).ok().map(|s| s.to_string());
                }
            }
            0 => {
                // varint
                loop {
                    if i >= data.len() {
                        return None;
                    }
                    let b2 = data[i];
                    i += 1;
                    if b2 & 0x80 == 0 {
                        break;
                    }
                }
            }
            1 => {
                // 64-bit
                i += 8;
            }
            5 => {
                // 32-bit
                i += 4;
            }
            _ => {
                // 非法 wire type，停止扫描
                break;
            }
        }
    }
    None
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
            dashboard_url: "https://windsurf.com/subscription/usage",
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
        assert_eq!(
            data.quotas[0].label_spec,
            crate::models::QuotaLabelSpec::Raw("Daily Quota".to_string())
        );
        assert!((data.quotas[0].used - 59.0).abs() < 0.01); // 100 - 41 = 59
        assert_eq!(
            data.quotas[1].label_spec,
            crate::models::QuotaLabelSpec::Raw("Weekly Quota".to_string())
        );
        assert!((data.quotas[1].used - 30.0).abs() < 0.01); // 100 - 70 = 30
    }

    #[test]
    fn test_build_quota_from_cached_fresh() {
        let future_ts = chrono::Utc::now().timestamp() + 3600;
        let q = build_quota_from_cached("Daily Quota", Some(41.0), Some(future_ts)).unwrap();
        assert_eq!(
            q.label_spec,
            crate::models::QuotaLabelSpec::Raw("Daily Quota".to_string())
        );
        assert!((q.used - 59.0).abs() < 0.01);
        assert!(matches!(
            q.detail_spec,
            Some(QuotaDetailSpec::ResetAt { .. })
        )); // 有倒计时
    }

    #[test]
    fn test_build_quota_from_cached_stale_resets_to_full() {
        // reset 时间已过期 → 配额已重置，应视为 0% used
        let past_ts = chrono::Utc::now().timestamp() - 3600;
        let q = build_quota_from_cached("Daily Quota", Some(41.0), Some(past_ts)).unwrap();
        assert_eq!(
            q.label_spec,
            crate::models::QuotaLabelSpec::Raw("Daily Quota".to_string())
        );
        assert!((q.used - 0.0).abs() < 0.01); // 过期后重置为 0% used
        assert!(q.detail_spec.is_none()); // 不展示过期的倒计时
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

    fn make_proto_with_email(email: &str) -> Vec<u8> {
        // 构造包含 field=7 (email) 的最小 protobuf
        // tag = (7 << 3) | 2 = 0x3a
        let mut data = vec![0x3a, email.len() as u8];
        data.extend_from_slice(email.as_bytes());
        data
    }

    fn make_auth_status_with_proto_email(email: &str) -> String {
        let proto = make_proto_with_email(email);
        let b64 = STANDARD.encode(&proto);
        format!(r#"{{"userStatusProtoBinaryBase64":"{}"}}"#, b64)
    }

    #[test]
    fn test_extract_email_old_format_direct_field() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO ItemTable (key, value) VALUES (?1, ?2)",
            [
                "windsurfAuthStatus",
                r#"{"email":"user@example.com","userStatusProtoBinaryBase64":""}"#,
            ],
        )
        .unwrap();

        let spec = test_windsurf_spec();
        let email = extract_email_from_auth_status(&conn, &spec);
        assert_eq!(email, Some("user@example.com".to_string()));
    }

    #[test]
    fn test_extract_email_new_format_protobuf_scan() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
            [],
        )
        .unwrap();
        // 新格式：没有顶层 email 字段，email 在 protobuf 里
        let auth_status_json = make_auth_status_with_proto_email("user@example.com");
        conn.execute(
            "INSERT INTO ItemTable (key, value) VALUES (?1, ?2)",
            ["windsurfAuthStatus", &auth_status_json],
        )
        .unwrap();

        let spec = test_windsurf_spec();
        let email = extract_email_from_auth_status(&conn, &spec);
        assert_eq!(email, Some("user@example.com".to_string()));
    }

    #[test]
    fn test_extract_email_new_format_with_bad_wire_before_email() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
            [],
        )
        .unwrap();
        // 构造一个 protobuf：先有非法 wire type，再有 email
        // 宽容扫描应在遇到非法字节时停止，返回 None
        let mut bad_proto = vec![0x07]; // 非法 wire type
        bad_proto.extend_from_slice(&make_proto_with_email("user@example.com"));
        let b64 = STANDARD.encode(&bad_proto);
        let auth_status_json = format!(r#"{{"userStatusProtoBinaryBase64":"{}"}}"#, b64);
        conn.execute(
            "INSERT INTO ItemTable (key, value) VALUES (?1, ?2)",
            ["windsurfAuthStatus", &auth_status_json],
        )
        .unwrap();

        let spec = test_windsurf_spec();
        let email = extract_email_from_auth_status(&conn, &spec);
        assert!(email.is_none());
    }

    #[test]
    fn test_extract_email_returns_none_when_auth_status_absent() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
            [],
        )
        .unwrap();

        let spec = test_windsurf_spec();
        let email = extract_email_from_auth_status(&conn, &spec);
        assert!(email.is_none());
    }

    #[test]
    fn test_read_via_cached_plan_info_with_proto_email() {
        // 集成测试：cachedPlanInfo + auth_status（带 protobuf email）→ email 被提取
        let future_daily = chrono::Utc::now().timestamp() + 3600;
        let future_weekly = chrono::Utc::now().timestamp() + 86400 * 5;
        let plan_json = format!(
            r#"{{"planName":"Pro","quotaUsage":{{"dailyRemainingPercent":60,"weeklyRemainingPercent":80,"dailyResetAtUnix":{},"weeklyResetAtUnix":{}}}}}"#,
            future_daily, future_weekly
        );
        let auth_status_json = make_auth_status_with_proto_email("integrated@example.com");

        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO ItemTable (key, value) VALUES (?1, ?2)",
            ["windsurf.settings.cachedPlanInfo", &plan_json],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO ItemTable (key, value) VALUES (?1, ?2)",
            ["windsurfAuthStatus", &auth_status_json],
        )
        .unwrap();

        let spec = test_windsurf_spec();
        let data = read_via_cached_plan_info(&conn, &spec).unwrap();
        assert_eq!(
            data.account_email,
            Some("integrated@example.com".to_string())
        );
        assert_eq!(data.account_tier, Some("Pro".to_string()));
        assert_eq!(data.quotas.len(), 2);
    }

    #[test]
    fn test_extract_string_field_permissive_finds_email() {
        // 构造一个最小 protobuf：field=7 wire=2，内容为 "hello@example.com"
        // tag byte = (7 << 3) | 2 = 58 = 0x3a
        let email = b"hello@example.com";
        let mut data = vec![0x3a, email.len() as u8];
        data.extend_from_slice(email);

        let result = extract_string_field_permissive(&data, 7);
        assert_eq!(result, Some("hello@example.com".to_string()));
    }

    #[test]
    fn test_extract_string_field_permissive_stops_on_bad_wire() {
        // field=3 wire=2 内容 "name"，然后 field=0 wire=7（非法），然后 field=7 wire=2 内容 "email"
        // 应该在非法 wire type 处停止，返回 None（email 在非法字节之后）
        let data = vec![
            0x1a, 4, b'n', b'a', b'm', b'e', // field 3, wire 2, "name"
            0x07, // field 0, wire 7 (非法)
            0x3a, 5, b'e', b'm', b'a', b'i', b'l', // field 7, wire 2, "email"
        ];

        // email 在非法字节之后，找不到
        let result = extract_string_field_permissive(&data, 7);
        assert!(result.is_none());
    }

    #[test]
    fn test_extract_string_field_permissive_before_bad_wire() {
        // field=7 wire=2 内容 "hello@example.com"，然后 field=0 wire=7（非法）
        // email 在非法字节之前，应该能找到
        let email = b"hello@example.com";
        let mut data = vec![0x3a, email.len() as u8];
        data.extend_from_slice(email);
        data.push(0x07); // 非法 wire type

        let result = extract_string_field_permissive(&data, 7);
        assert_eq!(result, Some("hello@example.com".to_string()));
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

    #[test]
    fn test_read_via_cached_plan_info_weekly_quota_null_when_full() {
        // 模拟新版 Windsurf 行为：周限额已满时 weeklyRemainingPercent 为 null
        // 但 weeklyResetAtUnix 仍然存在
        let future_daily = chrono::Utc::now().timestamp() + 3600;
        let future_weekly = chrono::Utc::now().timestamp() + 86400 * 3; // 3天后重置

        // JSON 中 weeklyRemainingPercent 为 null（限额已满时的实际情况）
        let plan_json = r#"{"planName":"Pro","quotaUsage":{"dailyRemainingPercent":60,"weeklyRemainingPercent":null,"dailyResetAtUnix":"#.to_string()
            + &future_daily.to_string()
            + r#","weeklyResetAtUnix":"#
            + &future_weekly.to_string()
            + r#"}}"#;

        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO ItemTable (key, value) VALUES (?1, ?2)",
            ["windsurf.settings.cachedPlanInfo", &plan_json],
        )
        .unwrap();

        let spec = test_windsurf_spec();
        let data = read_via_cached_plan_info(&conn, &spec).unwrap();

        // 应该返回两个配额：日和周
        assert_eq!(data.quotas.len(), 2);

        // 日配额正常：40% used (100-60)
        let daily = &data.quotas[0];
        assert_eq!(
            daily.label_spec,
            crate::models::QuotaLabelSpec::Raw("Daily Quota".to_string())
        );
        assert!((daily.used - 40.0).abs() < 0.01);

        // 周配额：weeklyRemainingPercent 为 null 时应推断为 100% used（已满）
        let weekly = &data.quotas[1];
        assert_eq!(
            weekly.label_spec,
            crate::models::QuotaLabelSpec::Raw("Weekly Quota".to_string())
        );
        assert!((weekly.used - 100.0).abs() < 0.01); // 已满
        assert!(matches!(
            weekly.detail_spec,
            Some(QuotaDetailSpec::ResetAt { .. })
        ));
    }
}
