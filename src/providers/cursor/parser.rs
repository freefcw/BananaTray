use crate::models::{QuotaDetailSpec, QuotaInfo, QuotaLabelSpec, QuotaType};
use crate::providers::ProviderError;
use crate::utils::time_utils;
use anyhow::Result;

const USD_CENTS_PER_DOLLAR: f64 = 100.0;

pub(super) fn parse_usage_response(body: &str) -> Result<Vec<QuotaInfo>> {
    let json: serde_json::Value = serde_json::from_str(body)
        .map_err(|_| ProviderError::parse_failed("usage-summary response"))?;

    let mut quotas = Vec::new();

    let membership_type = json
        .get("membershipType")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    let is_unlimited = json
        .get("isUnlimited")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let reset_at = json
        .get("billingCycleEnd")
        .and_then(|v| v.as_str())
        .and_then(time_utils::parse_iso8601_to_epoch)
        .map(|epoch_secs| QuotaDetailSpec::ResetAt { epoch_secs });

    let is_free_tier = membership_type.eq_ignore_ascii_case("free");
    let tier_label = membership_type.to_uppercase();

    if is_unlimited {
        quotas.push(QuotaInfo::with_details(
            QuotaLabelSpec::MonthlyTier {
                tier: tier_label.clone(),
            },
            0.0,
            1.0,
            QuotaType::General,
            Some(QuotaDetailSpec::Unlimited),
        ));
        return Ok(quotas);
    }

    let individual_usage = json.get("individualUsage");
    let limit_type = json.get("limitType").and_then(|v| v.as_str()).unwrap_or("");

    quotas.extend(parse_plan_quotas(
        individual_usage,
        &tier_label,
        is_free_tier,
        reset_at.clone(),
    ));
    if let Some(on_demand_quota) = parse_credit_quota(
        individual_usage.and_then(|usage| usage.get("onDemand")),
        QuotaLabelSpec::OnDemand,
        reset_at.clone(),
    ) {
        quotas.push(on_demand_quota);
    }
    if limit_type == "team" {
        if let Some(team_quota) = parse_credit_quota(
            json.get("teamUsage")
                .and_then(|usage| usage.get("onDemand")),
            QuotaLabelSpec::Team,
            reset_at,
        ) {
            quotas.push(team_quota);
        }
    }

    if quotas.is_empty() {
        return Err(ProviderError::no_data().into());
    }

    Ok(quotas)
}

/// 解析 plan 配额：优先拆成 Auto（自有模型）与 API（三方模型）两池；
/// 若响应缺少百分比字段，再回退到单一 used/limit 月度档。
///
/// free 档没有 included API 额度池（该额度只属于 Pro/Pro+/Ultra），
/// 上游仍会返回恒为 0 的 `apiPercentUsed`，因此 free 只在该值为 0 时隐藏 API 池。
fn parse_plan_quotas(
    individual_usage: Option<&serde_json::Value>,
    tier: &str,
    is_free_tier: bool,
    reset_at: Option<QuotaDetailSpec>,
) -> Vec<QuotaInfo> {
    let plan = match individual_usage.and_then(|usage| usage.get("plan")) {
        Some(plan) => plan,
        None => return Vec::new(),
    };
    if !plan
        .get("enabled")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        return Vec::new();
    }

    let auto_percent = plan
        .get("autoPercentUsed")
        .and_then(serde_json::Value::as_f64);
    // free 档的 API 池恒为 0，展示它只是噪音；但一旦真出现非零用量
    // （free 账号开通 on-demand、或 Cursor 调整 free 政策），仍要展示，
    // 隐藏真实消耗比多一条 0% 更危险。
    let api_percent = plan
        .get("apiPercentUsed")
        .and_then(serde_json::Value::as_f64)
        .filter(|percent| !is_free_tier || *percent > 0.0);

    if auto_percent.is_some() || api_percent.is_some() {
        let mut quotas = Vec::with_capacity(2);
        if let Some(used_percent) = auto_percent {
            quotas.push(QuotaInfo::from_used_percent(
                QuotaLabelSpec::SubscriptionUsage {
                    plan: tier.to_string(),
                    pool: "auto".into(),
                },
                used_percent,
                QuotaType::General,
                reset_at.clone(),
            ));
        }
        if let Some(used_percent) = api_percent {
            quotas.push(QuotaInfo::from_used_percent(
                QuotaLabelSpec::SubscriptionUsage {
                    plan: tier.to_string(),
                    pool: "api".into(),
                },
                used_percent,
                QuotaType::General,
                reset_at,
            ));
        }
        return quotas;
    }

    parse_legacy_plan_quota(plan, tier, is_free_tier, reset_at)
        .into_iter()
        .collect()
}

fn parse_legacy_plan_quota(
    plan: &serde_json::Value,
    tier: &str,
    is_free_tier: bool,
    reset_at: Option<QuotaDetailSpec>,
) -> Option<QuotaInfo> {
    let used = plan
        .get("used")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.0);
    let declared_limit = plan
        .get("limit")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.0);
    // free 档的 `breakdown.total`（included + bonus）会随用量增长，不是固定上限，
    // 拿它当 limit 会算出漂移的百分比；宁可无数据也不展示错误数字。
    let breakdown_limit = if is_free_tier {
        0.0
    } else {
        plan.get("breakdown")
            .and_then(|breakdown| breakdown.get("total"))
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0)
    };
    let limit = if declared_limit > 0.0 {
        declared_limit
    } else {
        breakdown_limit
    };
    if limit <= 0.0 {
        return None;
    }

    let used = if declared_limit == 0.0 {
        plan.get("totalPercentUsed")
            .and_then(serde_json::Value::as_f64)
            .map(|percent| (percent * limit / 100.0).round())
            .unwrap_or(used)
    } else {
        used
    };
    Some(QuotaInfo::with_details(
        QuotaLabelSpec::MonthlyTier {
            tier: tier.to_string(),
        },
        used,
        limit,
        QuotaType::General,
        reset_at,
    ))
}

fn parse_credit_quota(
    usage: Option<&serde_json::Value>,
    label: QuotaLabelSpec,
    reset_at: Option<QuotaDetailSpec>,
) -> Option<QuotaInfo> {
    let usage = usage?;
    if !usage
        .get("enabled")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        return None;
    }

    let used = usage
        .get("used")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.0);
    let limit = usage
        .get("limit")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.0);
    (limit > 0.0).then(|| {
        QuotaInfo::with_details(
            label,
            used / USD_CENTS_PER_DOLLAR,
            limit / USD_CENTS_PER_DOLLAR,
            QuotaType::Credit,
            reset_at,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn evaluate_quota(
        engine: &mut crate::application::AlertEngine,
        quotas: &[QuotaInfo],
        rules: &crate::models::QuotaRules,
    ) -> Vec<crate::application::QuotaNotificationEvent> {
        let provider = crate::models::ProviderId::BuiltIn(crate::models::ProviderKind::Cursor);
        engine.evaluate(crate::application::QuotaObservation {
            provider_id: &provider,
            provider_name: "Cursor",
            quotas,
            rules,
            usage_step_pct: 0,
        })
    }

    #[test]
    fn test_parse_unlimited_plan() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let body = r#"{"membershipType":"pro","isUnlimited":true,"billingCycleEnd":"2026-05-01T00:00:00Z"}"#;
        let quotas = parse_usage_response(body).unwrap();
        assert_eq!(quotas.len(), 1);
        assert_eq!(quotas[0].detail_spec, Some(QuotaDetailSpec::Unlimited));
    }

    #[test]
    fn test_parse_auto_and_api_pools() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let body = r#"{
            "membershipType":"pro",
            "isUnlimited":false,
            "billingCycleEnd":"2026-05-01T00:00:00Z",
            "individualUsage":{
                "plan":{
                    "enabled":true,
                    "used":40,
                    "limit":100,
                    "autoPercentUsed":12.5,
                    "apiPercentUsed":80,
                    "totalPercentUsed":55
                }
            }
        }"#;
        let quotas = parse_usage_response(body).unwrap();
        assert_eq!(quotas.len(), 2);
        assert_eq!(
            quotas[0].label_spec,
            QuotaLabelSpec::SubscriptionUsage {
                plan: "PRO".into(),
                pool: "auto".into(),
            }
        );
        assert!((quotas[0].used - 12.5).abs() < f64::EPSILON);
        assert!((quotas[0].limit - 100.0).abs() < f64::EPSILON);
        assert_eq!(
            quotas[1].label_spec,
            QuotaLabelSpec::SubscriptionUsage {
                plan: "PRO".into(),
                pool: "api".into(),
            }
        );
        assert!((quotas[1].used - 80.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_team_and_ondemand_with_pools() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let body = r#"{
            "membershipType":"business",
            "isUnlimited":false,
            "billingCycleEnd":"2026-05-01T00:00:00Z",
            "limitType":"team",
            "individualUsage":{
                "plan":{
                    "enabled":true,
                    "used":40,
                    "limit":100,
                    "autoPercentUsed":0,
                    "apiPercentUsed":100,
                    "totalPercentUsed":100
                },
                "onDemand":{"enabled":true,"used":5,"limit":20}
            },
            "teamUsage":{"onDemand":{"enabled":true,"used":10,"limit":50}}
        }"#;
        let quotas = parse_usage_response(body).unwrap();
        assert_eq!(quotas.len(), 4);
        assert_eq!(
            quotas[0].label_spec,
            QuotaLabelSpec::SubscriptionUsage {
                plan: "BUSINESS".into(),
                pool: "auto".into(),
            }
        );
        assert_eq!(
            quotas[1].label_spec,
            QuotaLabelSpec::SubscriptionUsage {
                plan: "BUSINESS".into(),
                pool: "api".into(),
            }
        );
        assert_eq!(quotas[2].label_spec, QuotaLabelSpec::OnDemand);
        assert_eq!(quotas[3].label_spec, QuotaLabelSpec::Team);
    }

    #[test]
    fn test_parse_legacy_plan_without_percent_pools() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let body = r#"{
            "membershipType":"pro",
            "isUnlimited":false,
            "billingCycleEnd":"2026-05-01T00:00:00Z",
            "individualUsage":{
                "plan":{"enabled":true,"used":40,"limit":100}
            }
        }"#;
        let quotas = parse_usage_response(body).unwrap();
        assert_eq!(quotas.len(), 1);
        assert_eq!(
            quotas[0].label_spec,
            QuotaLabelSpec::MonthlyTier { tier: "PRO".into() }
        );
        assert!((quotas[0].used - 40.0).abs() < f64::EPSILON);
        assert!((quotas[0].limit - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_empty_response_returns_error() {
        let body = r#"{"membershipType":"free","isUnlimited":false}"#;
        assert!(parse_usage_response(body).is_err());
    }

    /// free 档真实响应：`used` / `limit` 恒为 0，额度信息只在 percent 字段里，
    /// `apiPercentUsed` 恒为 0（free 没有 API 额度池），因此只展示 Auto 池。
    #[test]
    fn test_parse_free_tier_skips_api_pool() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let body = r#"{
            "membershipType":"free",
            "isUnlimited":false,
            "billingCycleEnd":"2026-05-01T00:00:00Z",
            "individualUsage":{
                "plan":{
                    "enabled":true,
                    "used":0,
                    "limit":0,
                    "remaining":0,
                    "breakdown":{"included":0,"bonus":11,"total":11},
                    "autoPercentUsed":11,
                    "apiPercentUsed":0,
                    "totalPercentUsed":5.5
                },
                "onDemand":{"enabled":false,"used":0,"limit":null,"remaining":null}
            }
        }"#;
        let quotas = parse_usage_response(body).unwrap();
        assert_eq!(quotas.len(), 1);
        assert_eq!(
            quotas[0].label_spec,
            QuotaLabelSpec::SubscriptionUsage {
                plan: "FREE".into(),
                pool: "auto".into(),
            }
        );
        assert!((quotas[0].used - 11.0).abs() < f64::EPSILON);
        assert!((quotas[0].limit - 100.0).abs() < f64::EPSILON);
    }

    /// free 档一旦真有 API 用量就必须展示：隐藏真实消耗比多一条 0% 更危险。
    #[test]
    fn test_parse_free_tier_keeps_nonzero_api_pool() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let body = r#"{
            "membershipType":"free",
            "isUnlimited":false,
            "individualUsage":{
                "plan":{"enabled":true,"autoPercentUsed":11,"apiPercentUsed":7.5}
            }
        }"#;
        let quotas = parse_usage_response(body).unwrap();
        assert_eq!(quotas.len(), 2);
        assert_eq!(
            quotas[1].label_spec,
            QuotaLabelSpec::SubscriptionUsage {
                plan: "FREE".into(),
                pool: "api".into(),
            }
        );
        assert!((quotas[1].used - 7.5).abs() < f64::EPSILON);
    }

    /// 付费档的 `apiPercentUsed = 0` 仍要展示：0 是真实用量，不是"无此池"。
    #[test]
    fn test_parse_paid_tier_keeps_zero_api_pool() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let body = r#"{
            "membershipType":"pro",
            "isUnlimited":false,
            "individualUsage":{
                "plan":{"enabled":true,"autoPercentUsed":30,"apiPercentUsed":0}
            }
        }"#;
        let quotas = parse_usage_response(body).unwrap();
        assert_eq!(quotas.len(), 2);
        assert_eq!(
            quotas[1].label_spec,
            QuotaLabelSpec::SubscriptionUsage {
                plan: "PRO".into(),
                pool: "api".into(),
            }
        );
        assert!((quotas[1].used - 0.0).abs() < f64::EPSILON);
    }

    /// free 档缺少 percent 字段时不得用会随用量增长的 `breakdown.total` 当 limit。
    #[test]
    fn test_parse_free_tier_ignores_breakdown_total_as_limit() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let body = r#"{
            "membershipType":"free",
            "isUnlimited":false,
            "individualUsage":{
                "plan":{
                    "enabled":true,
                    "used":0,
                    "limit":0,
                    "breakdown":{"included":0,"bonus":11,"total":11}
                }
            }
        }"#;
        assert!(parse_usage_response(body).is_err());
    }

    /// 付费档仍保留 `breakdown.total` 回退，避免回归已有行为。
    #[test]
    fn test_parse_paid_tier_uses_breakdown_total_fallback() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let body = r#"{
            "membershipType":"pro",
            "isUnlimited":false,
            "individualUsage":{
                "plan":{
                    "enabled":true,
                    "used":0,
                    "limit":0,
                    "breakdown":{"included":20,"bonus":0,"total":20},
                    "totalPercentUsed":50
                }
            }
        }"#;
        let quotas = parse_usage_response(body).unwrap();
        assert_eq!(quotas.len(), 1);
        assert_eq!(
            quotas[0].label_spec,
            QuotaLabelSpec::MonthlyTier { tier: "PRO".into() }
        );
        assert!((quotas[0].used - 10.0).abs() < f64::EPSILON);
        assert!((quotas[0].limit - 20.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_credit_quota_scales_api_cents_to_dollars() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let cases = [
            (
                "personal on-demand",
                r#"{"membershipType":"pro","isUnlimited":false,"individualUsage":{"onDemand":{"enabled":true,"used":1100,"limit":2000}}}"#,
                r#"{"membershipType":"pro","isUnlimited":false,"individualUsage":{"onDemand":{"enabled":true,"used":1950,"limit":2000}}}"#,
                QuotaLabelSpec::OnDemand,
                "on-demand",
            ),
            (
                "team on-demand",
                r#"{"membershipType":"business","isUnlimited":false,"limitType":"team","teamUsage":{"onDemand":{"enabled":true,"used":1100,"limit":2000}}}"#,
                r#"{"membershipType":"business","isUnlimited":false,"limitType":"team","teamUsage":{"onDemand":{"enabled":true,"used":1950,"limit":2000}}}"#,
                QuotaLabelSpec::Team,
                "team",
            ),
        ];
        for (name, body_low, body_high, label, key) in cases {
            let rules = crate::models::QuotaRules::default();
            let quotas_low = parse_usage_response(body_low).unwrap();
            let quotas_high = parse_usage_response(body_high).unwrap();
            assert_eq!(quotas_low.len(), 1, "{name}");
            assert_eq!(quotas_high.len(), 1, "{name}");

            let low = &quotas_low[0];
            let high = &quotas_high[0];
            assert_eq!((low.used, low.limit), (11.0, 20.0), "{name}");
            assert_eq!((high.used, high.limit), (19.5, 20.0), "{name}");
            assert_eq!(low.label_spec, label, "{name}");
            assert_eq!(low.stable_key, key, "{name}");
            assert_eq!(low.quota_type, QuotaType::Credit, "{name}");

            let m_low = low.threshold_measurement().unwrap();
            let m_high = high.threshold_measurement().unwrap();
            assert_eq!(
                m_low.unit,
                crate::models::QuotaThresholdUnit::Currency,
                "{name}"
            );
            assert!((m_low.remaining - 9.0).abs() < 1e-9, "{name}");
            assert!((m_high.remaining - 0.5).abs() < 1e-9, "{name}");
            assert_eq!(
                low.status_level(&rules),
                crate::models::StatusLevel::Yellow,
                "{name}"
            );
            assert_eq!(
                high.status_level(&rules),
                crate::models::StatusLevel::Red,
                "{name}"
            );
            assert!((high.percent_remaining() - 2.5).abs() < 1e-9, "{name}");

            let mut engine = crate::application::AlertEngine::new();
            let baseline = evaluate_quota(&mut engine, &quotas_low, &rules);
            assert!(baseline.is_empty(), "{name}");
            let events = evaluate_quota(&mut engine, &quotas_high, &rules);
            assert_eq!(events.len(), 1, "{name}");
            match &events[0] {
                crate::application::QuotaNotificationEvent::LowQuota { quota, .. } => {
                    assert_eq!(quota.stable_key, key, "{name}");
                    assert!(
                        (quota.threshold_measurement().unwrap().remaining - 0.5).abs() < 1e-9,
                        "{name}"
                    );
                }
                other => panic!("{name}: expected LowQuota, got {other:?}"),
            }
        }
    }

    #[test]
    fn test_on_demand_guard_skips_disabled_and_invalid_limits() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        for on_demand in [
            r#"{"enabled":false,"used":1100,"limit":2000}"#,
            r#"{"enabled":true,"used":1100,"limit":null}"#,
            r#"{"enabled":true,"used":1100,"limit":0}"#,
            r#"{"enabled":true,"used":1100,"limit":-50}"#,
        ] {
            let body = format!(
                r#"{{"membershipType":"pro","isUnlimited":false,"individualUsage":{{"onDemand":{on_demand}}}}}"#
            );
            assert!(
                parse_usage_response(&body).is_err(),
                "onDemand {on_demand} should produce no quota"
            );
        }
    }

    #[test]
    fn test_credit_quota_cent_boundary_uses_comparison_scale() {
        use crate::models::{QuotaRules, QuotaThresholdUnit, QuotaThresholds, StatusLevel};

        let _locale_guard = crate::i18n::test_locale_guard("en");
        let mut rules = QuotaRules::default();
        rules.currency = QuotaThresholds {
            warning: 0.5,
            critical: 0.01,
            notify: 0.01,
        };
        rules
            .currency
            .validate(QuotaThresholdUnit::Currency)
            .unwrap();

        let shapes = [
            (
                "personal on-demand",
                QuotaLabelSpec::OnDemand,
                "on-demand",
                false,
            ),
            ("team on-demand", QuotaLabelSpec::Team, "team", true),
        ];
        for (name, label, key, is_team) in shapes {
            for budget in [20002_i64, 100007, 1000007] {
                let body = |used: i64| {
                    let usage = serde_json::json!({"enabled": true, "used": used, "limit": budget});
                    let root = if is_team {
                        serde_json::json!({
                            "membershipType": "business", "isUnlimited": false, "limitType": "team",
                            "teamUsage": {"onDemand": usage},
                        })
                    } else {
                        serde_json::json!({
                            "membershipType": "pro", "isUnlimited": false,
                            "individualUsage": {"onDemand": usage},
                        })
                    };
                    root.to_string()
                };
                let baseline = parse_usage_response(&body(budget - 102)).unwrap();
                assert_eq!(baseline.len(), 1, "{name} budget={budget}");
                let b = &baseline[0];
                assert_eq!(b.label_spec, label, "{name} budget={budget}");
                assert_eq!(b.stable_key, key, "{name} budget={budget}");
                let m_b = b.threshold_measurement().unwrap();
                assert_eq!(m_b.unit, QuotaThresholdUnit::Currency, "{name}b{budget}");
                assert!(
                    (m_b.remaining - 1.02).abs() < 1e-9,
                    "{name} budget={budget}"
                );
                assert_eq!(
                    b.status_level(&rules),
                    StatusLevel::Green,
                    "{name}b{budget}"
                );

                for (cents, expected) in [(1_i64, StatusLevel::Red), (2, StatusLevel::Yellow)] {
                    let current = parse_usage_response(&body(budget - cents)).unwrap();
                    assert_eq!(current.len(), 1, "{name} budget={budget} cents={cents}");
                    let m = current[0].threshold_measurement().unwrap();
                    assert_eq!(m.unit, QuotaThresholdUnit::Currency, "{name}b{budget}");
                    assert!(
                        (m.remaining - cents as f64 / 100.0).abs() < 1e-9,
                        "{name} budget={budget} cents={cents}"
                    );
                    if budget == 20002 && cents == 1 {
                        assert!(m.remaining > 0.01, "{name}: unrounded {:.17}", m.remaining);
                    }
                    let status = current[0].status_level(&rules);
                    assert_eq!(status, expected, "{name} budget={budget} cents={cents}");

                    let mut engine = crate::application::AlertEngine::new();
                    let base_events = evaluate_quota(&mut engine, &baseline, &rules);
                    assert!(base_events.is_empty(), "{name} budget={budget}");
                    let events = evaluate_quota(&mut engine, &current, &rules);
                    if expected == StatusLevel::Red {
                        let [crate::application::QuotaNotificationEvent::LowQuota { quota, .. }] =
                            events.as_slice()
                        else {
                            panic!("{name} budget={budget}: expected LowQuota, got {events:?}")
                        };
                        assert_eq!(quota.stable_key, key, "{name} budget={budget}");
                        let rem = quota.threshold_measurement().unwrap().remaining;
                        assert!((rem - 0.01).abs() < 1e-9, "{name} budget={budget}");
                    } else {
                        assert!(events.is_empty(), "{name} budget={budget} cents={cents}");
                    }
                }
            }
        }
    }
}
