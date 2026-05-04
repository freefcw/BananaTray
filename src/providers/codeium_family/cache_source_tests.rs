use super::auth_status::{
    decode_user_status_payload, extract_email_from_auth_status, extract_string_field_permissive,
};
use super::cached_plan::{
    build_quota_from_cached, parse_cached_plan_info, read_via_cached_plan_info, CachedQuotaKind,
};
use super::sqlite_store::{cache_db_path_candidates, query_cached_plan_info};
use super::*;
use super::{ensure_cache_fresh, select_fresh_cache_db_path};
use crate::models::QuotaDetailSpec;
use base64::{engine::general_purpose::STANDARD, Engine};
use rusqlite::Connection;
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
        cache_db_config_relative_path: "Windsurf/User/globalStorage/state.vscdb",
        auth_status_key_candidates: &["windsurfAuthStatus", "antigravityAuthStatus"],
        process_markers: &["--ide_name windsurf", "/windsurf/", "/windsurf.app/"],
        cached_plan_info_key_candidates: &["windsurf.settings.cachedPlanInfo"],
        cache_max_age_secs: 0,
    }
}

#[test]
fn test_cache_db_candidates_include_platform_config_path() {
    let spec = test_windsurf_spec();
    let candidates = cache_db_path_candidates(&spec);

    let expected_suffix = std::path::Path::new("Windsurf/User/globalStorage/state.vscdb");
    assert!(candidates
        .iter()
        .any(|path| path.ends_with(expected_suffix)));
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
    assert!(matches!(err, ProviderError::ParseFailed { .. }));
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
        crate::models::QuotaLabelSpec::Daily
    );
    assert!((data.quotas[0].used - 59.0).abs() < 0.01); // 100 - 41 = 59
    assert_eq!(
        data.quotas[1].label_spec,
        crate::models::QuotaLabelSpec::Weekly
    );
    assert!((data.quotas[1].used - 30.0).abs() < 0.01); // 100 - 70 = 30
}

#[test]
fn test_build_quota_from_cached_fresh() {
    let future_ts = chrono::Utc::now().timestamp() + 3600;
    let q = build_quota_from_cached(CachedQuotaKind::Daily, Some(41.0), Some(future_ts)).unwrap();
    assert_eq!(q.label_spec, crate::models::QuotaLabelSpec::Daily);
    assert_eq!(q.stable_key, "daily-quota");
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
    let q = build_quota_from_cached(CachedQuotaKind::Daily, Some(41.0), Some(past_ts)).unwrap();
    assert_eq!(q.label_spec, crate::models::QuotaLabelSpec::Daily);
    assert!((q.used - 0.0).abs() < 0.01); // 过期后重置为 0% used
    assert!(q.detail_spec.is_none()); // 不展示过期的倒计时
}

#[test]
fn test_build_quota_from_cached_no_reset_time() {
    // 没有 reset 时间 → 按原始数据展示
    let q = build_quota_from_cached(CachedQuotaKind::Weekly, Some(70.0), None).unwrap();
    assert!((q.used - 30.0).abs() < 0.01);
}

#[test]
fn test_build_quota_from_cached_no_percent() {
    // 没有百分比数据 → 返回 None
    let q = build_quota_from_cached(CachedQuotaKind::Daily, None, Some(9999999999));
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
    assert!(matches!(err, ProviderError::ParseFailed { .. }));
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
    assert_eq!(daily.label_spec, crate::models::QuotaLabelSpec::Daily);
    assert!((daily.used - 40.0).abs() < 0.01);

    // 周配额：weeklyRemainingPercent 为 null 时应推断为 100% used（已满）
    let weekly = &data.quotas[1];
    assert_eq!(weekly.label_spec, crate::models::QuotaLabelSpec::Weekly);
    assert!((weekly.used - 100.0).abs() < 0.01); // 已满
    assert!(matches!(
        weekly.detail_spec,
        Some(QuotaDetailSpec::ResetAt { .. })
    ));
}

fn spec_with_max_age(secs: u64) -> CodeiumFamilySpec {
    let mut spec = test_windsurf_spec();
    spec.cache_max_age_secs = secs;
    spec
}

#[test]
fn test_ensure_cache_fresh_skips_when_threshold_zero() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.vscdb");
    std::fs::write(&path, b"").unwrap();
    // 即使文件不存在 mtime，也不该返回错误（threshold=0 直接 ok）
    assert!(ensure_cache_fresh(&path, &spec_with_max_age(0)).is_ok());
}

#[test]
fn test_ensure_cache_fresh_accepts_recent_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.vscdb");
    std::fs::write(&path, b"").unwrap();
    // 刚写入的文件 mtime ≈ now，3 小时阈值内应通过
    assert!(ensure_cache_fresh(&path, &spec_with_max_age(3 * 3600)).is_ok());
}

#[test]
fn test_ensure_cache_fresh_rejects_stale_file() {
    use std::time::{Duration, SystemTime};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.vscdb");
    let file = std::fs::File::create(&path).unwrap();

    // 把 mtime 倒回 4 小时前（std::fs 自 1.75 起支持 set_modified）
    let four_hours_ago = SystemTime::now() - Duration::from_secs(4 * 3600);
    file.set_modified(four_hours_ago).unwrap();
    drop(file);

    let err = ensure_cache_fresh(&path, &spec_with_max_age(3 * 3600)).unwrap_err();
    assert!(matches!(err, ProviderError::Unavailable { .. }));
}

#[test]
fn test_ensure_cache_fresh_missing_file_returns_unavailable() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("does-not-exist.vscdb");
    let err = ensure_cache_fresh(&missing, &spec_with_max_age(3 * 3600)).unwrap_err();
    assert!(matches!(err, ProviderError::Unavailable { .. }));
}

#[test]
fn test_ensure_cache_fresh_wal_overrides_stale_main_db() {
    use std::time::{Duration, SystemTime};
    // 模拟 SQLite WAL 模式的真实场景：
    // 主 DB 文件 mtime 很老（上次 checkpoint 距今很久），但 -wal sidecar 是新的，
    // 应判 fresh，否则会把"还在活跃写入"的 cache 误报为 stale。
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("state.vscdb");
    let wal = dir.path().join("state.vscdb-wal");

    let main_file = std::fs::File::create(&db).unwrap();
    main_file
        .set_modified(SystemTime::now() - Duration::from_secs(10 * 3600))
        .unwrap();
    drop(main_file);

    // WAL 刚刚被写入 → 当前活跃
    std::fs::File::create(&wal).unwrap();

    assert!(
        ensure_cache_fresh(&db, &spec_with_max_age(3 * 3600)).is_ok(),
        "WAL sidecar 是最新的，应视为 cache 仍新鲜"
    );
}

#[test]
fn test_ensure_cache_fresh_journal_overrides_stale_main_db() {
    use std::time::{Duration, SystemTime};
    // 同上，但走非 WAL 模式（rollback journal）路径
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("state.vscdb");
    let journal = dir.path().join("state.vscdb-journal");

    let main_file = std::fs::File::create(&db).unwrap();
    main_file
        .set_modified(SystemTime::now() - Duration::from_secs(10 * 3600))
        .unwrap();
    drop(main_file);

    std::fs::File::create(&journal).unwrap();

    assert!(ensure_cache_fresh(&db, &spec_with_max_age(3 * 3600)).is_ok());
}

#[test]
fn test_ensure_cache_fresh_all_sidecars_stale_returns_unavailable() {
    use std::time::{Duration, SystemTime};
    // 主 DB 与 WAL 都老 → 真 stale，应拒绝
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("state.vscdb");
    let wal = dir.path().join("state.vscdb-wal");
    let four_hours_ago = SystemTime::now() - Duration::from_secs(4 * 3600);

    for path in [&db, &wal] {
        let f = std::fs::File::create(path).unwrap();
        f.set_modified(four_hours_ago).unwrap();
    }

    let err = ensure_cache_fresh(&db, &spec_with_max_age(3 * 3600)).unwrap_err();
    let ProviderError::Unavailable { raw_detail, .. } = err else {
        panic!("expected Unavailable");
    };
    let detail = raw_detail.expect("stale error should carry raw_detail");
    // 错误信息应同时包含 stale 关键字、路径与行动建议
    assert!(detail.contains("stale"), "detail: {detail}");
    assert!(
        detail.contains("Open"),
        "detail should suggest action: {detail}"
    );
    assert!(
        detail.contains("state.vscdb"),
        "detail should include cache path: {detail}"
    );
}

#[test]
fn test_select_fresh_cache_db_path_skips_stale_candidate() {
    use std::time::{Duration, SystemTime};
    let dir = tempfile::tempdir().unwrap();
    let stale = dir.path().join("old").join("state.vscdb");
    let fresh = dir.path().join("new").join("state.vscdb");
    std::fs::create_dir_all(stale.parent().unwrap()).unwrap();
    std::fs::create_dir_all(fresh.parent().unwrap()).unwrap();

    let stale_file = std::fs::File::create(&stale).unwrap();
    stale_file
        .set_modified(SystemTime::now() - Duration::from_secs(4 * 3600))
        .unwrap();
    drop(stale_file);
    std::fs::File::create(&fresh).unwrap();

    let selected =
        select_fresh_cache_db_path(&spec_with_max_age(3 * 3600), vec![stale, fresh.clone()])
            .unwrap();

    assert_eq!(selected, fresh);
}

#[test]
fn test_select_fresh_cache_db_path_returns_stale_error_when_all_existing_candidates_stale() {
    use std::time::{Duration, SystemTime};
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing").join("state.vscdb");
    let stale = dir.path().join("old").join("state.vscdb");
    std::fs::create_dir_all(stale.parent().unwrap()).unwrap();

    let stale_file = std::fs::File::create(&stale).unwrap();
    stale_file
        .set_modified(SystemTime::now() - Duration::from_secs(4 * 3600))
        .unwrap();
    drop(stale_file);

    let err =
        select_fresh_cache_db_path(&spec_with_max_age(3 * 3600), vec![missing, stale]).unwrap_err();
    let ProviderError::Unavailable { raw_detail, .. } = err else {
        panic!("expected Unavailable");
    };
    assert!(
        raw_detail
            .as_deref()
            .is_some_and(|detail| detail.contains("stale")),
        "expected stale detail, got {raw_detail:?}"
    );
}

#[test]
fn test_ensure_cache_fresh_age_at_threshold_is_accepted() {
    use std::time::{Duration, SystemTime};
    // 边界：age == threshold 应通过（实现是 `>` 而非 `>=`）
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("state.vscdb");
    let f = std::fs::File::create(&db).unwrap();
    // 留 2 秒裕量避免到达断言时已超过阈值
    f.set_modified(SystemTime::now() - Duration::from_secs(3 * 3600 - 2))
        .unwrap();
    drop(f);

    assert!(ensure_cache_fresh(&db, &spec_with_max_age(3 * 3600)).is_ok());
}

#[test]
fn test_read_refresh_data_short_circuits_on_stale_cache() {
    use std::time::{Duration, SystemTime};
    // end-to-end：cache 文件 stale 时，read_refresh_data 应在 SQLite 打开前
    // 直接返回 Unavailable，而不是继续解析。
    //
    // 我们没法直接驱动 read_refresh_data 走真实 cache_db_path（依赖 dirs），
    // 所以验证更下沉的行为：ensure_cache_fresh 在打开 connection 前就拒绝。
    // 这保证了 stale short-circuit 的语义。
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("state.vscdb");
    let f = std::fs::File::create(&db).unwrap();
    f.set_modified(SystemTime::now() - Duration::from_secs(24 * 3600))
        .unwrap();
    drop(f);

    let spec = spec_with_max_age(3 * 3600);
    // 即使 SQLite 文件根本不是合法 DB，也不应触发"打开失败"——
    // freshness 闸应在更前面拦下。
    let err = ensure_cache_fresh(&db, &spec).unwrap_err();
    assert!(matches!(err, ProviderError::Unavailable { .. }));
}
