use crate::models::{QuotaInfo, QuotaType, RefreshData};
use crate::providers::ProviderError;
use anyhow::{bail, Result};
use regex::Regex;

use super::schema::{JsonQuotaRule, ParserDef, QuotaTypeDef, RegexQuotaRule};

/// 根据 ParserDef 从原始响应中提取 RefreshData
pub fn extract(parser: &ParserDef, raw: &str) -> Result<RefreshData> {
    match parser {
        ParserDef::Json {
            account_email,
            account_tier,
            quotas,
        } => extract_json(raw, account_email, account_tier, quotas),
        ParserDef::Regex {
            account_email,
            quotas,
        } => extract_regex(raw, account_email, quotas),
    }
}

// ============================================================================
// JSON 提取
// ============================================================================

fn extract_json(
    raw: &str,
    email_path: &Option<String>,
    tier_path: &Option<String>,
    rules: &[JsonQuotaRule],
) -> Result<RefreshData> {
    let json: serde_json::Value = serde_json::from_str(raw)
        .map_err(|_| ProviderError::parse_failed("invalid JSON response"))?;

    let account_email = email_path.as_ref().and_then(|p| json_string(&json, p));
    let account_tier = tier_path.as_ref().and_then(|p| json_string(&json, p));

    let mut quotas = Vec::new();
    for rule in rules {
        let used = json_f64(&json, &rule.used).ok_or_else(|| {
            ProviderError::parse_failed(&format!(
                "JSON path '{}' not found or not numeric",
                rule.used
            ))
        })?;
        let limit = json_f64(&json, &rule.limit).ok_or_else(|| {
            ProviderError::parse_failed(&format!(
                "JSON path '{}' not found or not numeric",
                rule.limit
            ))
        })?;
        let detail = rule.detail.as_ref().and_then(|p| json_string(&json, p));
        let quota_type = map_quota_type(&rule.quota_type);

        quotas.push(QuotaInfo::with_details(
            &rule.label,
            used,
            limit,
            quota_type,
            detail,
        ));
    }

    if quotas.is_empty() {
        return Err(ProviderError::no_data().into());
    }

    Ok(RefreshData::with_account(
        quotas,
        account_email,
        account_tier,
    ))
}

/// 点分路径 JSON 值提取（如 "data.usage.used"）
///
/// 支持语法：
/// - `field.nested.value` — 对象字段访问
/// - `array.0.value` — 数组索引访问
fn json_navigate<'a>(root: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    let mut current = root;
    for segment in path.split('.') {
        if segment.is_empty() {
            continue;
        }
        if let Ok(idx) = segment.parse::<usize>() {
            current = current.get(idx)?;
        } else {
            current = current.get(segment)?;
        }
    }
    Some(current)
}

fn json_f64(root: &serde_json::Value, path: &str) -> Option<f64> {
    let val = json_navigate(root, path)?;
    if let Some(n) = val.as_f64() {
        return Some(n);
    }
    // 兼容字符串数字（如 "256"）
    val.as_str().and_then(|s| s.parse::<f64>().ok())
}

fn json_string(root: &serde_json::Value, path: &str) -> Option<String> {
    let val = json_navigate(root, path)?;
    val.as_str().map(|s| s.to_string())
}

// ============================================================================
// Regex 提取
// ============================================================================

fn extract_regex(
    raw: &str,
    email_pattern: &Option<String>,
    rules: &[RegexQuotaRule],
) -> Result<RefreshData> {
    let account_email = email_pattern.as_ref().and_then(|pattern| {
        Regex::new(pattern).ok().and_then(|re| {
            re.captures(raw)
                .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
        })
    });

    let mut quotas = Vec::new();
    for rule in rules {
        let re = Regex::new(&rule.pattern)
            .map_err(|e| ProviderError::parse_failed(&format!("invalid regex: {}", e)))?;

        if let Some(caps) = re.captures(raw) {
            let used_str = caps.get(rule.used_group).map(|m| m.as_str());
            let limit_str = caps.get(rule.limit_group).map(|m| m.as_str());

            let used: f64 = used_str.and_then(|s| s.parse().ok()).ok_or_else(|| {
                ProviderError::parse_failed(&format!(
                    "regex group {} not found or not numeric in pattern '{}'",
                    rule.used_group, rule.pattern
                ))
            })?;

            let limit: f64 = limit_str.and_then(|s| s.parse().ok()).ok_or_else(|| {
                ProviderError::parse_failed(&format!(
                    "regex group {} not found or not numeric in pattern '{}'",
                    rule.limit_group, rule.pattern
                ))
            })?;

            let quota_type = map_quota_type(&rule.quota_type);
            quotas.push(QuotaInfo::with_details(
                &rule.label,
                used,
                limit,
                quota_type,
                None,
            ));
        }
    }

    if quotas.is_empty() {
        bail!("no quota data matched by regex rules");
    }

    Ok(RefreshData::with_account(quotas, account_email, None))
}

fn map_quota_type(def: &QuotaTypeDef) -> QuotaType {
    match def {
        QuotaTypeDef::Session => QuotaType::Session,
        QuotaTypeDef::Weekly => QuotaType::Weekly,
        QuotaTypeDef::Credit => QuotaType::Credit,
        QuotaTypeDef::General => QuotaType::General,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── json_navigate ───────────────────────────

    #[test]
    fn test_json_navigate_simple() {
        let json: serde_json::Value =
            serde_json::from_str(r#"{"usage":{"used":10,"limit":100}}"#).unwrap();
        assert_eq!(json_f64(&json, "usage.used"), Some(10.0));
        assert_eq!(json_f64(&json, "usage.limit"), Some(100.0));
    }

    #[test]
    fn test_json_navigate_array_index() {
        let json: serde_json::Value =
            serde_json::from_str(r#"{"items":[{"val":1},{"val":2}]}"#).unwrap();
        assert_eq!(json_f64(&json, "items.0.val"), Some(1.0));
        assert_eq!(json_f64(&json, "items.1.val"), Some(2.0));
    }

    #[test]
    fn test_json_navigate_string_number() {
        let json: serde_json::Value =
            serde_json::from_str(r#"{"used":"256","limit":"1024"}"#).unwrap();
        assert_eq!(json_f64(&json, "used"), Some(256.0));
        assert_eq!(json_f64(&json, "limit"), Some(1024.0));
    }

    #[test]
    fn test_json_navigate_missing_path() {
        let json: serde_json::Value = serde_json::from_str(r#"{"a":1}"#).unwrap();
        assert_eq!(json_f64(&json, "b.c"), None);
    }

    // ── extract_json ────────────────────────────

    #[test]
    fn test_extract_json_full() {
        let raw = r#"{
            "user": {"email": "test@example.com"},
            "plan": {"name": "Pro"},
            "usage": {"used": 75, "limit": 100, "reset": "2026-05-01"}
        }"#;

        let rules = vec![JsonQuotaRule {
            label: "Monthly".to_string(),
            used: "usage.used".to_string(),
            limit: "usage.limit".to_string(),
            quota_type: QuotaTypeDef::General,
            detail: Some("usage.reset".to_string()),
        }];

        let data = extract_json(
            raw,
            &Some("user.email".to_string()),
            &Some("plan.name".to_string()),
            &rules,
        )
        .unwrap();

        assert_eq!(data.account_email.as_deref(), Some("test@example.com"));
        assert_eq!(data.account_tier.as_deref(), Some("Pro"));
        assert_eq!(data.quotas.len(), 1);
        assert_eq!(data.quotas[0].label, "Monthly");
        assert_eq!(data.quotas[0].used, 75.0);
        assert_eq!(data.quotas[0].limit, 100.0);
        assert_eq!(data.quotas[0].detail_text.as_deref(), Some("2026-05-01"));
    }

    #[test]
    fn test_extract_json_no_rules() {
        let raw = r#"{"empty": true}"#;
        assert!(extract_json(raw, &None, &None, &[]).is_err());
    }

    #[test]
    fn test_extract_json_missing_used_path_returns_error() {
        let raw = r#"{"usage": {"limit": 100}}"#;
        let rules = vec![JsonQuotaRule {
            label: "Test".to_string(),
            used: "usage.used".to_string(),
            limit: "usage.limit".to_string(),
            quota_type: QuotaTypeDef::General,
            detail: None,
        }];
        let err = extract_json(raw, &None, &None, &rules).unwrap_err();
        assert!(err.to_string().contains("usage.used"));
    }

    #[test]
    fn test_extract_json_missing_limit_path_returns_error() {
        let raw = r#"{"usage": {"used": 50}}"#;
        let rules = vec![JsonQuotaRule {
            label: "Test".to_string(),
            used: "usage.used".to_string(),
            limit: "usage.limit".to_string(),
            quota_type: QuotaTypeDef::General,
            detail: None,
        }];
        let err = extract_json(raw, &None, &None, &rules).unwrap_err();
        assert!(err.to_string().contains("usage.limit"));
    }

    #[test]
    fn test_extract_json_non_numeric_value_returns_error() {
        let raw = r#"{"usage": {"used": "abc", "limit": 100}}"#;
        let rules = vec![JsonQuotaRule {
            label: "Test".to_string(),
            used: "usage.used".to_string(),
            limit: "usage.limit".to_string(),
            quota_type: QuotaTypeDef::General,
            detail: None,
        }];
        let err = extract_json(raw, &None, &None, &rules).unwrap_err();
        assert!(err.to_string().contains("usage.used"));
    }

    #[test]
    fn test_extract_json_invalid_json() {
        let err = extract_json("not json", &None, &None, &[]).unwrap_err();
        assert!(err.to_string().contains("invalid JSON"));
    }

    // ── extract_regex ───────────────────────────

    #[test]
    fn test_extract_regex_basic() {
        let raw = "Credits: 25/100 remaining\n";
        let rules = vec![RegexQuotaRule {
            label: "Credits".to_string(),
            pattern: r"Credits:\s*(\d+)/(\d+)".to_string(),
            used_group: 1,
            limit_group: 2,
            quota_type: QuotaTypeDef::General,
        }];

        let data = extract_regex(raw, &None, &rules).unwrap();
        assert_eq!(data.quotas.len(), 1);
        assert_eq!(data.quotas[0].used, 25.0);
        assert_eq!(data.quotas[0].limit, 100.0);
    }

    #[test]
    fn test_extract_regex_with_email() {
        let raw = "Signed in as user@test.com\nUsage: 50/200\n";
        let email_pattern = Some(r"Signed in as\s+(\S+)".to_string());
        let rules = vec![RegexQuotaRule {
            label: "Usage".to_string(),
            pattern: r"Usage:\s*(\d+)/(\d+)".to_string(),
            used_group: 1,
            limit_group: 2,
            quota_type: QuotaTypeDef::Weekly,
        }];

        let data = extract_regex(raw, &email_pattern, &rules).unwrap();
        assert_eq!(data.account_email.as_deref(), Some("user@test.com"));
        assert_eq!(data.quotas[0].used, 50.0);
    }

    #[test]
    fn test_extract_regex_no_match() {
        let raw = "no matching content";
        let rules = vec![RegexQuotaRule {
            label: "Test".to_string(),
            pattern: r"(\d+)/(\d+)".to_string(),
            used_group: 1,
            limit_group: 2,
            quota_type: QuotaTypeDef::General,
        }];
        assert!(extract_regex(raw, &None, &rules).is_err());
    }

    #[test]
    fn test_extract_regex_invalid_pattern() {
        let rules = vec![RegexQuotaRule {
            label: "Bad".to_string(),
            pattern: r"[invalid".to_string(),
            used_group: 1,
            limit_group: 2,
            quota_type: QuotaTypeDef::General,
        }];
        assert!(extract_regex("test", &None, &rules).is_err());
    }

    #[test]
    fn test_extract_regex_bad_group_index_returns_error() {
        let raw = "Credits: 25/100";
        let rules = vec![RegexQuotaRule {
            label: "Test".to_string(),
            pattern: r"Credits:\s*(\d+)/(\d+)".to_string(),
            used_group: 5, // 不存在的 group
            limit_group: 2,
            quota_type: QuotaTypeDef::General,
        }];
        let err = extract_regex(raw, &None, &rules).unwrap_err();
        assert!(err.to_string().contains("group 5"));
    }

    // ── map_quota_type ──────────────────────────

    #[test]
    fn test_map_quota_type() {
        assert!(matches!(
            map_quota_type(&QuotaTypeDef::Session),
            QuotaType::Session
        ));
        assert!(matches!(
            map_quota_type(&QuotaTypeDef::Weekly),
            QuotaType::Weekly
        ));
        assert!(matches!(
            map_quota_type(&QuotaTypeDef::Credit),
            QuotaType::Credit
        ));
        assert!(matches!(
            map_quota_type(&QuotaTypeDef::General),
            QuotaType::General
        ));
    }
}
