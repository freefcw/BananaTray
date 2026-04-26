use super::*;
use crate::models::test_helpers::{make_test_metadata, make_test_provider};
use crate::models::{ProviderId, ProviderKind};

// ========================================================================
// QuotaType::stable_key
// ========================================================================

#[test]
fn quota_type_stable_key_is_language_independent() {
    assert_eq!(QuotaType::Session.stable_key(), "session");
    assert_eq!(QuotaType::Weekly.stable_key(), "weekly");
    assert_eq!(
        QuotaType::ModelSpecific("Opus".into()).stable_key(),
        "model:Opus"
    );
    assert_eq!(QuotaType::Credit.stable_key(), "credit");
    assert_eq!(QuotaType::General.stable_key(), "general");
}

// ========================================================================
// 基础计算测试
// ========================================================================

#[test]
fn test_quota_percentage() {
    let q1 = QuotaInfo::new("test", 50.0, 100.0);
    assert_eq!(q1.percentage(), 50.0);

    let q2 = QuotaInfo::new("test", 150.0, 100.0); // 溢出
    assert_eq!(q2.percentage(), 150.0); // 不 clamp，返回实际值

    let q3 = QuotaInfo::new("test", 0.0, 0.0); // 除零
    assert_eq!(q3.percentage(), 0.0);
}

#[test]
fn test_quota_percent_remaining() {
    let q1 = QuotaInfo::new("test", 30.0, 100.0);
    assert_eq!(q1.percent_remaining(), 70.0);

    let q2 = QuotaInfo::new("test", 100.0, 100.0); // 已用完
    assert_eq!(q2.percent_remaining(), 0.0);

    let q3 = QuotaInfo::new("test", 150.0, 100.0); // 超出
    assert_eq!(q3.percent_remaining(), -50.0); // 返回负数

    let q4 = QuotaInfo::new("test", 0.0, 0.0); // 除零
    assert_eq!(q4.percent_remaining(), 0.0);
}

#[test]
fn test_quota_percent_remaining_precision() {
    // 测试浮点精度
    let q = QuotaInfo::new("test", 33.333, 100.0);
    assert!((q.percent_remaining() - 66.667).abs() < 0.01);
}

// ========================================================================
// 状态判断测试（基于 status_level 单一真理来源）
// ========================================================================

#[test]
fn test_status_level_green() {
    let q = QuotaInfo::new("green", 40.0, 100.0);
    assert_eq!(q.status_level(), StatusLevel::Green);
    assert!(q.is_healthy());
    assert!(!q.is_warning());
    assert!(!q.is_critical());
    assert!(!q.is_depleted());
}

#[test]
fn test_status_level_green_boundary() {
    // 正好 50% 剩余 = Yellow（因为 > 50 才是 Green）
    let q_50_remaining = QuotaInfo::new("boundary", 50.0, 100.0);
    assert_eq!(q_50_remaining.status_level(), StatusLevel::Yellow);

    // 49.9% 剩余 = Yellow
    let q_49_9 = QuotaInfo::new("almost_green", 50.1, 100.0);
    assert_eq!(q_49_9.status_level(), StatusLevel::Yellow);

    // 50.1% 剩余 = Green
    let q_50_1 = QuotaInfo::new("just_green", 49.9, 100.0);
    assert_eq!(q_50_1.status_level(), StatusLevel::Green);
}

#[test]
fn test_status_level_yellow() {
    // 50% 使用 = 50% 剩余 -> Yellow 边界
    let q_50 = QuotaInfo::new("yellow", 50.0, 100.0);
    assert_eq!(q_50.status_level(), StatusLevel::Yellow);
    assert!(!q_50.is_healthy());
    assert!(q_50.is_warning());
    assert!(!q_50.is_critical());

    // 80% 使用 = 20% 剩余 -> Yellow 边界
    let q_80 = QuotaInfo::new("yellow_edge", 80.0, 100.0);
    assert_eq!(q_80.status_level(), StatusLevel::Yellow);
    assert!(!q_80.is_critical()); // 20% 是 Yellow 边界，不是 critical
}

#[test]
fn test_status_level_red() {
    // 81% 使用 = 19% 剩余 -> Red
    let q = QuotaInfo::new("red", 81.0, 100.0);
    assert_eq!(q.status_level(), StatusLevel::Red);
    assert!(!q.is_healthy());
    assert!(!q.is_warning());
    assert!(q.is_critical()); // Red 但未耗尽
    assert!(!q.is_depleted());
}

#[test]
fn test_status_level_red_boundary() {
    // 正好 20% 剩余 = Yellow（因为 >= 20 是 Yellow）
    let q_20 = QuotaInfo::new("boundary", 80.0, 100.0);
    assert_eq!(q_20.status_level(), StatusLevel::Yellow);

    // 19.9% 剩余 = Red
    let q_19_9 = QuotaInfo::new("just_red", 80.1, 100.0);
    assert_eq!(q_19_9.status_level(), StatusLevel::Red);
}

#[test]
fn test_depletion() {
    let q_normal = QuotaInfo::new("normal", 50.0, 100.0);
    assert!(!q_normal.is_depleted());

    let q_exact = QuotaInfo::new("exact", 100.0, 100.0);
    assert!(q_exact.is_depleted());

    let q_exceeded = QuotaInfo::new("exceeded", 150.0, 100.0);
    assert!(q_exceeded.is_depleted());

    // 耗尽时 critical 为 false（因为耗尽不是"接近耗尽"）
    assert!(!q_exact.is_critical());
    assert!(!q_exceeded.is_critical());
}

#[test]
fn test_critical_vs_depleted() {
    // critical 是 Red 且未耗尽
    let q_critical = QuotaInfo::new("critical", 85.0, 100.0);
    assert!(q_critical.is_critical());
    assert!(!q_critical.is_depleted());

    // 耗尽不是 critical
    let q_depleted = QuotaInfo::new("depleted", 100.0, 100.0);
    assert!(!q_depleted.is_critical());
    assert!(q_depleted.is_depleted());
}

#[test]
fn test_status_level_ordering() {
    assert!(StatusLevel::Green < StatusLevel::Yellow);
    assert!(StatusLevel::Yellow < StatusLevel::Red);
    assert_eq!(
        [StatusLevel::Red, StatusLevel::Green, StatusLevel::Yellow]
            .iter()
            .max(),
        Some(&StatusLevel::Red)
    );
}

// ========================================================================
// 类型判断测试
// ========================================================================

#[test]
fn test_quota_type_checks() {
    let q_session = QuotaInfo::with_details("Session", 50.0, 100.0, QuotaType::Session, None);
    assert!(q_session.is_session());
    assert!(!q_session.is_weekly());
    assert!(!q_session.is_credit());

    let q_weekly = QuotaInfo::with_details("Weekly", 50.0, 100.0, QuotaType::Weekly, None);
    assert!(q_weekly.is_weekly());

    let q_credit = QuotaInfo::with_details("Credit", 5.0, 20.0, QuotaType::Credit, None);
    assert!(q_credit.is_credit());

    let q_model = QuotaInfo::with_details(
        "Opus",
        50.0,
        100.0,
        QuotaType::ModelSpecific("Opus".into()),
        None,
    );
    assert!(!q_model.is_session());
    assert!(!q_model.is_weekly());
    assert!(!q_model.is_credit());
}

// ========================================================================
// 余额模式测试
// ========================================================================

#[test]
fn test_balance_only_construction() {
    let q = QuotaInfo::balance_only("Balance", 10.0, Some(3.0), QuotaType::Credit, None);
    assert!(q.is_balance_only());
    assert!((q.remaining_balance.unwrap() - 10.0).abs() < f64::EPSILON);
    assert!((q.used - 3.0).abs() < f64::EPSILON);
    assert!((q.limit - 0.0).abs() < f64::EPSILON);
}

#[test]
fn test_balance_only_without_used() {
    let q = QuotaInfo::balance_only("Balance", 5.0, None, QuotaType::Credit, None);
    assert!(q.is_balance_only());
    assert!((q.used - 0.0).abs() < f64::EPSILON);
}

#[test]
fn test_balance_only_is_not_set_for_normal() {
    let q = QuotaInfo::new("Normal", 30.0, 100.0);
    assert!(!q.is_balance_only());
    assert!(q.remaining_balance.is_none());
}

#[test]
fn test_balance_only_status_level() {
    // >= $5 → Green
    let q_green = QuotaInfo::balance_only("B", 10.0, None, QuotaType::Credit, None);
    assert_eq!(q_green.status_level(), StatusLevel::Green);

    let q_green_boundary = QuotaInfo::balance_only("B", 5.0, None, QuotaType::Credit, None);
    assert_eq!(q_green_boundary.status_level(), StatusLevel::Green);

    // $1 ~ $5 → Yellow
    let q_yellow = QuotaInfo::balance_only("B", 3.0, None, QuotaType::Credit, None);
    assert_eq!(q_yellow.status_level(), StatusLevel::Yellow);

    let q_yellow_boundary = QuotaInfo::balance_only("B", 1.0, None, QuotaType::Credit, None);
    assert_eq!(q_yellow_boundary.status_level(), StatusLevel::Yellow);

    // < $1 → Red
    let q_red = QuotaInfo::balance_only("B", 0.5, None, QuotaType::Credit, None);
    assert_eq!(q_red.status_level(), StatusLevel::Red);

    let q_red_zero = QuotaInfo::balance_only("B", 0.0, None, QuotaType::Credit, None);
    assert_eq!(q_red_zero.status_level(), StatusLevel::Red);
}

// ========================================================================
// 边界条件测试
// ========================================================================

#[test]
fn test_edge_cases() {
    // limit 为 0
    let q_zero_limit = QuotaInfo::new("zero", 10.0, 0.0);
    assert_eq!(q_zero_limit.percentage(), 0.0);
    assert_eq!(q_zero_limit.percent_remaining(), 0.0);
    assert!(!q_zero_limit.is_depleted()); // limit 为 0 时不算耗尽

    // used 和 limit 都为 0
    let q_both_zero = QuotaInfo::new("both_zero", 0.0, 0.0);
    assert_eq!(q_both_zero.percentage(), 0.0);
    assert!(!q_both_zero.is_depleted());

    // 负数 used（理论上不应该出现，但测试健壮性）
    // percent_remaining 会返回 > 100（因为剩余量是负的负数）
    let q_negative = QuotaInfo::new("negative", -10.0, 100.0);
    assert_eq!(q_negative.percentage(), -10.0); // 返回负数百分比
                                                // 浮点数精度：使用 approx_eq
    assert!((q_negative.percent_remaining() - 110.0).abs() < 0.01); // 剩余 110%
}

#[test]
fn test_percentage_mode() {
    let q_pct = QuotaInfo::new("percentage", 50.0, 100.0);
    assert!(q_pct.is_percentage_mode());

    let q_real = QuotaInfo::new("real", 5.0, 10.0);
    assert!(!q_real.is_percentage_mode());
}

// ========================================================================
// ProviderStatus 构造测试
// ========================================================================

#[test]
fn provider_status_new_supports_builtin_provider_ids() {
    let metadata = make_test_metadata(ProviderKind::Claude);
    let status = ProviderStatus::new(ProviderId::BuiltIn(ProviderKind::Claude), metadata);

    assert_eq!(
        status.provider_id,
        ProviderId::BuiltIn(ProviderKind::Claude)
    );
    assert_eq!(status.kind(), ProviderKind::Claude);
    assert_eq!(status.metadata.kind, ProviderKind::Claude);
    assert_eq!(status.connection, ConnectionStatus::Disconnected);
}

#[test]
fn provider_status_new_supports_custom_provider_ids() {
    let provider_id = ProviderId::Custom("demo:cli".to_string());
    let metadata = make_test_metadata(ProviderKind::Custom);
    let status = ProviderStatus::new(provider_id.clone(), metadata);

    assert_eq!(status.provider_id, provider_id);
    assert_eq!(status.kind(), ProviderKind::Custom);
    assert_eq!(status.metadata.kind, ProviderKind::Custom);
    assert_eq!(status.connection, ConnectionStatus::Disconnected);
}

// ========================================================================
// mark_refresh_failed 状态转换测试
// ========================================================================

fn make_provider(connection: ConnectionStatus) -> ProviderStatus {
    make_test_provider(ProviderKind::Claude, connection)
}

#[test]
fn mark_refresh_failed_sets_update_status() {
    let mut p = make_provider(ConnectionStatus::Connected);
    let failure = ProviderFailure {
        reason: FailureReason::Timeout,
        advice: None,
        raw_detail: None,
    };
    p.mark_refresh_failed(failure.clone(), ErrorKind::NetworkError);
    assert_eq!(p.update_status, Some(UpdateStatus::Failed));
    assert_eq!(p.last_failure, Some(failure));
    assert_eq!(p.connection, ConnectionStatus::Error);
    assert_eq!(p.error_kind, ErrorKind::NetworkError);
}

#[test]
fn mark_refresh_failed_with_existing_quotas_stays_connected() {
    let mut p = make_provider(ConnectionStatus::Connected);
    p.quotas = vec![QuotaInfo::new("test", 50.0, 100.0)];
    p.mark_refresh_failed(
        ProviderFailure {
            reason: FailureReason::Timeout,
            advice: None,
            raw_detail: None,
        },
        ErrorKind::NetworkError,
    );
    // 有旧配额数据时应保持 Connected（展示陈旧数据）
    assert_eq!(p.connection, ConnectionStatus::Connected);
    assert_eq!(p.update_status, Some(UpdateStatus::Failed));
}

#[test]
fn provider_status_prefers_runtime_source_label_after_success() {
    let mut p = make_provider(ConnectionStatus::Disconnected);
    let data = RefreshData::quotas_only(vec![QuotaInfo::new("test", 1.0, 100.0)])
        .with_source_label("seat api");

    p.mark_refresh_succeeded(data);

    assert_eq!(p.source_label(), "seat api");
}
