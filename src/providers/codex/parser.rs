use crate::models::{QuotaDetailSpec, QuotaInfo, QuotaLabelSpec, QuotaType};
use crate::providers::{ProviderError, ProviderResult};

/// 解析 Codex usage API 响应的结构化结果。
///
/// `plan_type` 对齐 CodexBar `CodexUsageResponse.planType`，由调用方与 JWT 中的
/// `chatgpt_plan_type` 合并后填入 `RefreshData::account_tier`。
#[derive(Debug, Clone, Default)]
pub(super) struct ParsedUsage {
    pub quotas: Vec<QuotaInfo>,
    pub plan_type: Option<String>,
}

/// Codex rate-limit 窗口的语义角色。
///
/// 与 CodexBar 的 `CodexRateWindowNormalizer` 保持一致：
/// 通过 `limit_window_seconds / 60` 得到 `window_minutes`，300 分钟 = 5h session，
/// 10080 分钟 = weekly。免费套餐只有 weekly 窗口，API 可能把它返回在 `primary_window`
/// 字段内，此时必须按 `window_minutes` 分类，而不是盲目按字段位置。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WindowRole {
    Session,
    Weekly,
}

impl WindowRole {
    pub(super) fn label_spec(self) -> QuotaLabelSpec {
        match self {
            WindowRole::Session => QuotaLabelSpec::Session,
            WindowRole::Weekly => QuotaLabelSpec::Weekly,
        }
    }

    pub(super) fn quota_type(self) -> QuotaType {
        match self {
            WindowRole::Session => QuotaType::Session,
            WindowRole::Weekly => QuotaType::Weekly,
        }
    }
}

/// 根据窗口分钟数判断窗口角色；若缺失或异常则回退到给定的默认角色。
pub(super) fn resolve_role_from_minutes(
    window_minutes: Option<i64>,
    default_role: WindowRole,
) -> WindowRole {
    match window_minutes {
        Some(300) => WindowRole::Session,
        Some(10080) => WindowRole::Weekly,
        _ => default_role,
    }
}

/// 根据 `limit_window_seconds` 判断窗口角色；若缺失或异常则回退到给定的默认角色。
fn resolve_role(limit_window_seconds: Option<i64>, default_role: WindowRole) -> WindowRole {
    resolve_role_from_minutes(limit_window_seconds.map(|s| s / 60), default_role)
}

fn build_window_quota(window: &serde_json::Value, default_role: WindowRole) -> QuotaInfo {
    let used = window
        .get("used_percent")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let reset_at = window.get("reset_at").and_then(|v| v.as_i64());
    let limit_window_seconds = window.get("limit_window_seconds").and_then(|v| v.as_i64());
    let role = resolve_role(limit_window_seconds, default_role);

    QuotaInfo::with_details(
        role.label_spec(),
        used,
        100.0,
        role.quota_type(),
        reset_at.map(|epoch_secs| QuotaDetailSpec::ResetAt { epoch_secs }),
    )
}

/// 解析 Codex usage API 响应。根据响应形态分派到 header 路径或 JSON 路径。
///
/// 注意：401/403 认证错误已在 http_client 层通过 `HttpError::HttpStatus` 结构化返回，
/// 不再需要在此处做字符串匹配。
pub(super) fn parse_usage_response(raw: &str) -> ProviderResult<ParsedUsage> {
    let (headers, body) = split_headers_and_body(raw);

    if let Some(quotas) = parse_header_response(headers) {
        // header 分支不携带 plan_type。
        return Ok(ParsedUsage {
            quotas,
            plan_type: None,
        });
    }

    parse_json_response(body)
}

/// 按 HTTP 响应格式把 raw 切成 headers 段与 body 段，容错两种换行风格。
fn split_headers_and_body(raw: &str) -> (&str, &str) {
    if let Some(idx) = raw.find("\r\n\r\n") {
        (&raw[..idx], raw[idx + 4..].trim())
    } else if let Some(idx) = raw.find("\n\n") {
        (&raw[..idx], raw[idx + 2..].trim())
    } else {
        ("", raw.trim())
    }
}

/// 解析 `x-codex-*` 自定义响应头。若未命中任何 codex header，返回 None 以触发 JSON 分支。
///
/// header 响应不携带 `limit_window_seconds`，因此只能按 primary/secondary 的命名语义映射。
fn parse_header_response(headers: &str) -> Option<Vec<QuotaInfo>> {
    let mut primary_percent: Option<f64> = None;
    let mut secondary_percent: Option<f64> = None;
    let mut credits_balance: Option<f64> = None;
    let mut matched = false;

    for line in headers.lines() {
        let lower = line.to_lowercase();
        let parse_value = || {
            line.split_once(':')
                .and_then(|(_, v)| v.trim().parse::<f64>().ok())
        };
        if lower.starts_with("x-codex-primary-used-percent:") {
            primary_percent = parse_value();
            matched = true;
        } else if lower.starts_with("x-codex-secondary-used-percent:") {
            secondary_percent = parse_value();
            matched = true;
        } else if lower.starts_with("x-codex-credits-balance:") {
            credits_balance = parse_value();
            matched = true;
        }
    }

    if !matched {
        return None;
    }

    let mut quotas = Vec::new();
    if let Some(primary) = primary_percent {
        quotas.push(QuotaInfo::with_details(
            QuotaLabelSpec::Session,
            primary,
            100.0,
            QuotaType::Session,
            None,
        ));
    }
    if let Some(secondary) = secondary_percent {
        quotas.push(QuotaInfo::with_details(
            QuotaLabelSpec::Weekly,
            secondary,
            100.0,
            QuotaType::Weekly,
            None,
        ));
    }
    if let Some(credits) = credits_balance {
        quotas.push(QuotaInfo::with_details(
            QuotaLabelSpec::Credits,
            0.0,
            credits,
            QuotaType::Credit,
            None,
        ));
    }
    Some(quotas)
}

/// 解析 JSON body：按 `limit_window_seconds` 正确区分 session / weekly，兼容免费套餐
/// 把 weekly 窗口放在 `primary_window` 字段的情况；从同响应提取 credits 余额与 plan_type。
fn parse_json_response(body: &str) -> ProviderResult<ParsedUsage> {
    if body.is_empty() {
        return Err(ProviderError::no_data());
    }

    let json: serde_json::Value = serde_json::from_str(body)
        .map_err(|_| ProviderError::parse_failed("usage API response"))?;

    let mut quotas = Vec::new();

    if let Some(rate_limit) = json.get("rate_limit") {
        if let Some(primary) = rate_limit.get("primary_window") {
            quotas.push(build_window_quota(primary, WindowRole::Session));
        }
        if let Some(secondary) = rate_limit.get("secondary_window") {
            quotas.push(build_window_quota(secondary, WindowRole::Weekly));
        }

        // 去重：若两个窗口被识别为相同角色（异常服务端返回），只保留第一个。
        if quotas.len() == 2 && quotas[0].quota_type == quotas[1].quota_type {
            quotas.truncate(1);
        }
    }

    if let Some(credits) = json.get("credits") {
        if let Some(balance) = read_credits_balance(credits) {
            quotas.push(QuotaInfo::balance_only(
                QuotaLabelSpec::Credits,
                balance,
                None,
                QuotaType::Credit,
                None,
            ));
        }
    }

    if quotas.is_empty() {
        return Err(ProviderError::no_data());
    }

    let plan_type = json
        .get("plan_type")
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    Ok(ParsedUsage { quotas, plan_type })
}

/// 读取 credits 余额，对齐 CodexBar 的 `CreditDetails`：
/// - `has_credits` 默认 false（保守，需要显式 true 才展示余额）
/// - `unlimited == true` 跳过
/// - `balance` 支持数字或字符串（与 CodexBar 的宽松解码一致）
fn read_credits_balance(credits: &serde_json::Value) -> Option<f64> {
    let has_credits = credits
        .get("has_credits")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let unlimited = credits
        .get("unlimited")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if !has_credits || unlimited {
        return None;
    }
    let balance_value = credits.get("balance")?;
    if let Some(f) = balance_value.as_f64() {
        return Some(f);
    }
    balance_value.as_str().and_then(|s| s.parse::<f64>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_headers_response() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let raw = "HTTP/1.1 200 OK\r\nx-codex-primary-used-percent: 25\r\nx-codex-secondary-used-percent: 80\r\nx-codex-credits-balance: 12.5\r\n\r\n";
        let quotas = parse_usage_response(raw).unwrap().quotas;
        assert_eq!(quotas.len(), 3);
        assert_eq!(quotas[0].label_spec, QuotaLabelSpec::Session);
        assert_eq!(quotas[1].label_spec, QuotaLabelSpec::Weekly);
        assert_eq!(quotas[2].label_spec, QuotaLabelSpec::Credits);
    }

    #[test]
    fn test_parse_json_response() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let raw = r#"{
            "rate_limit": {
                "primary_window": { "used_percent": 33, "reset_at": 1767225600 },
                "secondary_window": { "used_percent": 66, "reset_at": 1767312000 }
            }
        }"#;
        let quotas = parse_usage_response(raw).unwrap().quotas;
        assert_eq!(quotas.len(), 2);
        assert_eq!(quotas[0].used, 33.0);
        assert_eq!(quotas[1].used, 66.0);
        assert!(matches!(
            quotas[0].detail_spec,
            Some(QuotaDetailSpec::ResetAt { .. })
        ));
    }

    #[test]
    fn test_parse_json_classifies_by_limit_window_seconds() {
        // API 明确告知 primary_window 是 5h / secondary_window 是 weekly。
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let raw = r#"{
            "rate_limit": {
                "primary_window": {
                    "used_percent": 40,
                    "reset_at": 1767225600,
                    "limit_window_seconds": 18000
                },
                "secondary_window": {
                    "used_percent": 70,
                    "reset_at": 1767312000,
                    "limit_window_seconds": 604800
                }
            }
        }"#;
        let quotas = parse_usage_response(raw).unwrap().quotas;
        assert_eq!(quotas.len(), 2);
        assert_eq!(quotas[0].quota_type, QuotaType::Session);
        assert_eq!(quotas[1].quota_type, QuotaType::Weekly);
    }

    #[test]
    fn test_parse_json_free_plan_only_weekly_window() {
        // 免费套餐：API 可能仅返回 weekly 窗口并放在 primary_window 字段中。
        // 旧实现会错误标记为 Session/5h；新实现按 limit_window_seconds=604800 识别为 Weekly。
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let raw = r#"{
            "rate_limit": {
                "primary_window": {
                    "used_percent": 15,
                    "reset_at": 1767225600,
                    "limit_window_seconds": 604800
                }
            }
        }"#;
        let quotas = parse_usage_response(raw).unwrap().quotas;
        assert_eq!(quotas.len(), 1);
        assert_eq!(quotas[0].quota_type, QuotaType::Weekly);
        assert_eq!(quotas[0].label_spec, QuotaLabelSpec::Weekly);
        assert_eq!(quotas[0].used, 15.0);
    }

    #[test]
    fn test_parse_json_free_plan_weekly_in_secondary_only() {
        // 另一种等价形态：primary 缺失、weekly 在 secondary。
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let raw = r#"{
            "rate_limit": {
                "secondary_window": {
                    "used_percent": 25,
                    "reset_at": 1767312000,
                    "limit_window_seconds": 604800
                }
            }
        }"#;
        let quotas = parse_usage_response(raw).unwrap().quotas;
        assert_eq!(quotas.len(), 1);
        assert_eq!(quotas[0].quota_type, QuotaType::Weekly);
    }

    #[test]
    fn test_parse_json_credits_balance() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let raw = r#"{
            "rate_limit": {
                "primary_window": {
                    "used_percent": 10,
                    "reset_at": 1767225600,
                    "limit_window_seconds": 18000
                }
            },
            "credits": { "has_credits": true, "unlimited": false, "balance": 3.75 }
        }"#;
        let quotas = parse_usage_response(raw).unwrap().quotas;
        assert_eq!(quotas.len(), 2);
        let credit = quotas.iter().find(|q| q.is_credit()).expect("credits");
        assert!(credit.is_balance_only());
        assert!((credit.remaining_balance.unwrap() - 3.75).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_json_credits_unlimited_skipped() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let raw = r#"{
            "rate_limit": {
                "primary_window": {
                    "used_percent": 5,
                    "reset_at": 1767225600,
                    "limit_window_seconds": 18000
                }
            },
            "credits": { "has_credits": true, "unlimited": true, "balance": 0 }
        }"#;
        let quotas = parse_usage_response(raw).unwrap().quotas;
        assert!(quotas.iter().all(|q| !q.is_credit()));
    }

    #[test]
    fn test_resolve_role_fallback_when_limit_unknown() {
        // 无 limit_window_seconds：primary 默认 Session，secondary 默认 Weekly（向后兼容）。
        assert_eq!(resolve_role(None, WindowRole::Session), WindowRole::Session);
        assert_eq!(resolve_role(None, WindowRole::Weekly), WindowRole::Weekly);
        assert_eq!(
            resolve_role(Some(999_999), WindowRole::Session),
            WindowRole::Session
        );
    }

    #[test]
    fn test_resolve_role_exact_matches() {
        // 300 min = session；10080 min = weekly；即使 default 相反也应被覆盖。
        assert_eq!(
            resolve_role(Some(18_000), WindowRole::Weekly),
            WindowRole::Session
        );
        assert_eq!(
            resolve_role(Some(604_800), WindowRole::Session),
            WindowRole::Weekly
        );
    }

    #[test]
    fn test_parse_json_credits_balance_as_string() {
        // CodexBar 支持 balance 作为字符串返回；我们应同样宽松。
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let raw = r#"{
            "rate_limit": {
                "primary_window": {
                    "used_percent": 0,
                    "reset_at": 1767225600,
                    "limit_window_seconds": 18000
                }
            },
            "credits": { "has_credits": true, "unlimited": false, "balance": "7.25" }
        }"#;
        let quotas = parse_usage_response(raw).unwrap().quotas;
        let credit = quotas.iter().find(|q| q.is_credit()).expect("credits");
        assert!(credit.is_balance_only());
        assert!((credit.remaining_balance.unwrap() - 7.25).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_json_credits_has_credits_default_false() {
        // 对齐 CodexBar：has_credits 字段缺失时默认 false，不展示 credits 配额。
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let raw = r#"{
            "rate_limit": {
                "primary_window": {
                    "used_percent": 0,
                    "reset_at": 1767225600,
                    "limit_window_seconds": 18000
                }
            },
            "credits": { "balance": 99.0 }
        }"#;
        let quotas = parse_usage_response(raw).unwrap().quotas;
        assert!(quotas.iter().all(|q| !q.is_credit()));
    }

    #[test]
    fn test_parse_json_credits_has_credits_false_skipped() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let raw = r#"{
            "rate_limit": {
                "primary_window": {
                    "used_percent": 0,
                    "reset_at": 1767225600,
                    "limit_window_seconds": 18000
                }
            },
            "credits": { "has_credits": false, "balance": 10.0 }
        }"#;
        let quotas = parse_usage_response(raw).unwrap().quotas;
        assert!(quotas.iter().all(|q| !q.is_credit()));
    }

    #[test]
    fn test_parse_empty_body_returns_no_data() {
        // header 未命中 + body 空 → NoData
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let raw = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n";
        let err = parse_usage_response(raw).unwrap_err();
        assert!(matches!(err, ProviderError::NoData));
    }

    #[test]
    fn test_parse_invalid_json_returns_parse_failed() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let raw = "HTTP/1.1 200 OK\r\n\r\nnot json at all";
        let err = parse_usage_response(raw).unwrap_err();
        assert!(matches!(err, ProviderError::ParseFailed { .. }));
    }

    #[test]
    fn test_parse_json_empty_object_returns_no_data() {
        // 既无 rate_limit 也无 credits → NoData。
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let raw = "{}";
        let err = parse_usage_response(raw).unwrap_err();
        assert!(matches!(err, ProviderError::NoData));
    }

    #[test]
    fn test_parse_json_duplicate_role_deduplicated() {
        // 异常：两个窗口都声称自己是 weekly。应去重到 1 个。
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let raw = r#"{
            "rate_limit": {
                "primary_window": {
                    "used_percent": 10,
                    "reset_at": 1767225600,
                    "limit_window_seconds": 604800
                },
                "secondary_window": {
                    "used_percent": 20,
                    "reset_at": 1767312000,
                    "limit_window_seconds": 604800
                }
            }
        }"#;
        let quotas = parse_usage_response(raw).unwrap().quotas;
        assert_eq!(quotas.len(), 1);
        assert_eq!(quotas[0].quota_type, QuotaType::Weekly);
        assert_eq!(quotas[0].used, 10.0);
    }

    #[test]
    fn test_split_headers_and_body_lf_only() {
        let (headers, body) = split_headers_and_body("header: x\n\nBODY");
        assert_eq!(headers, "header: x");
        assert_eq!(body, "BODY");
    }

    #[test]
    fn test_split_headers_and_body_no_separator() {
        // 纯 body（无 header 段）：headers 为空、body 为原文。
        let (headers, body) = split_headers_and_body("{\"foo\":1}");
        assert_eq!(headers, "");
        assert_eq!(body, "{\"foo\":1}");
    }

    #[test]
    fn test_parse_json_plan_type_extracted() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let raw = r#"{
            "rate_limit": {
                "primary_window": {
                    "used_percent": 12,
                    "reset_at": 1767225600,
                    "limit_window_seconds": 18000
                }
            },
            "plan_type": "pro"
        }"#;
        let parsed = parse_usage_response(raw).unwrap();
        assert_eq!(parsed.plan_type.as_deref(), Some("pro"));
        assert_eq!(parsed.quotas.len(), 1);
    }

    #[test]
    fn test_parse_json_plan_type_absent_is_none() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let raw = r#"{
            "rate_limit": {
                "primary_window": {
                    "used_percent": 0,
                    "reset_at": 1767225600,
                    "limit_window_seconds": 18000
                }
            }
        }"#;
        let parsed = parse_usage_response(raw).unwrap();
        assert!(parsed.plan_type.is_none());
    }

    #[test]
    fn test_parse_json_plan_type_empty_or_whitespace_skipped() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let raw = r#"{
            "rate_limit": {
                "primary_window": {
                    "used_percent": 0,
                    "reset_at": 1767225600,
                    "limit_window_seconds": 18000
                }
            },
            "plan_type": "   "
        }"#;
        let parsed = parse_usage_response(raw).unwrap();
        assert!(parsed.plan_type.is_none());
    }

    #[test]
    fn test_parse_header_response_has_no_plan_type() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let raw = "HTTP/1.1 200 OK\r\nx-codex-primary-used-percent: 10\r\n\r\n";
        let parsed = parse_usage_response(raw).unwrap();
        assert!(parsed.plan_type.is_none());
        assert_eq!(parsed.quotas.len(), 1);
    }
}
