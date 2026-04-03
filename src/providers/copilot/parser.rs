use crate::models::{QuotaInfo, QuotaType, RefreshData};
use anyhow::{bail, Context, Result};
use rust_i18n::t;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct CopilotInternalResponse {
    copilot_plan: Option<String>,
    quota_snapshots: Option<QuotaSnapshots>,
}

/// GitHub /user API 响应（只取需要的字段）
#[derive(Debug, Deserialize)]
struct GitHubUserResponse {
    login: Option<String>,
    email: Option<String>,
}

#[derive(Debug, Deserialize)]
struct QuotaSnapshots {
    premium_interactions: Option<InteractionQuota>,
}

#[derive(Debug, Deserialize)]
struct InteractionQuota {
    entitlement: i32,
    remaining: i32,
    #[allow(dead_code)]
    percent_remaining: f64,
    unlimited: Option<bool>,
}

/// 从 /user API 响应中提取账户标识（优先 email，其次 login）
pub(super) fn parse_github_user(body: &str) -> Option<String> {
    let resp: GitHubUserResponse = serde_json::from_str(body).ok()?;
    // 优先用 email，没有则用 login（GitHub 用户名）
    resp.email
        .filter(|e| !e.is_empty())
        .or(resp.login.filter(|l| !l.is_empty()))
}

pub(super) fn parse_user_info_response(
    body: &str,
    status_code: &str,
    account_name: Option<String>,
) -> Result<RefreshData> {
    match status_code {
        "401" => {
            bail!("GitHub token is invalid or expired. Update your token in Settings → Providers.")
        }
        "403" => bail!("Token lacks required permissions. Use a Classic PAT with 'copilot' scope."),
        "404" => {
            bail!("Copilot not enabled for this account. Check your GitHub Copilot subscription.")
        }
        _ => {}
    }

    let resp: CopilotInternalResponse =
        serde_json::from_str(body).context("Failed to parse Copilot Internal API response.")?;

    let plan = resp.copilot_plan.unwrap_or_else(|| "unknown".to_string());
    let plan_label = capitalize_first(&plan);

    let quota = if let Some(snapshots) = resp.quota_snapshots {
        if let Some(interactions) = snapshots.premium_interactions {
            if interactions.unlimited.unwrap_or(false) {
                QuotaInfo::with_details(
                    t!("quota.label.premium_requests", plan = plan_label).to_string(),
                    0.0,
                    0.0,
                    QuotaType::General,
                    Some(t!("quota.label.unlimited").to_string()),
                )
            } else {
                let used = (interactions.entitlement - interactions.remaining).max(0) as f64;
                let limit = interactions.entitlement as f64;
                QuotaInfo::with_details(
                    t!("quota.label.premium_requests", plan = plan_label).to_string(),
                    used,
                    limit,
                    QuotaType::Weekly,
                    None,
                )
            }
        } else {
            QuotaInfo::with_details(
                t!("quota.label.chat_completions", plan = plan_label).to_string(),
                0.0,
                0.0,
                QuotaType::General,
                Some(t!("quota.label.unlimited").to_string()),
            )
        }
    } else {
        bail!("No quota data found in Copilot API response.");
    };

    Ok(RefreshData::with_account(
        vec![quota],
        account_name,
        Some(plan_label),
    ))
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_unlimited_plan() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let body = r#"{"copilot_plan":"pro","quota_snapshots":{"premium_interactions":{"entitlement":300,"remaining":300,"percent_remaining":100,"unlimited":true}}}"#;
        let data = parse_user_info_response(body, "200", None).unwrap();
        assert_eq!(data.account_tier.as_deref(), Some("Pro"));
        assert_eq!(data.quotas.len(), 1);
        assert_eq!(data.quotas[0].reset_at.as_deref(), Some("Unlimited"));
    }

    #[test]
    fn test_parse_limited_plan() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let body = r#"{"copilot_plan":"business","quota_snapshots":{"premium_interactions":{"entitlement":500,"remaining":125,"percent_remaining":25,"unlimited":false}}}"#;
        let data = parse_user_info_response(body, "200", None).unwrap();
        assert_eq!(data.account_tier.as_deref(), Some("Business"));
        assert_eq!(data.quotas[0].used, 375.0);
        assert_eq!(data.quotas[0].limit, 500.0);
    }

    #[test]
    fn test_parse_404_error() {
        let err = parse_user_info_response("{}", "404", None).unwrap_err();
        assert!(err.to_string().contains("Copilot not enabled"));
    }

    #[test]
    fn test_parse_with_account_name() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let body = r#"{"copilot_plan":"pro","quota_snapshots":{"premium_interactions":{"entitlement":300,"remaining":300,"percent_remaining":100,"unlimited":true}}}"#;
        let data = parse_user_info_response(body, "200", Some("octocat".to_string())).unwrap();
        assert_eq!(data.account_email.as_deref(), Some("octocat"));
        assert_eq!(data.account_tier.as_deref(), Some("Pro"));
    }

    #[test]
    fn test_parse_github_user_with_email() {
        let body = r#"{"login":"octocat","email":"octocat@github.com"}"#;
        assert_eq!(
            parse_github_user(body),
            Some("octocat@github.com".to_string())
        );
    }

    #[test]
    fn test_parse_github_user_fallback_to_login() {
        let body = r#"{"login":"octocat","email":null}"#;
        assert_eq!(parse_github_user(body), Some("octocat".to_string()));
    }

    #[test]
    fn test_parse_github_user_empty_email_fallback() {
        let body = r#"{"login":"octocat","email":""}"#;
        assert_eq!(parse_github_user(body), Some("octocat".to_string()));
    }

    #[test]
    fn test_parse_github_user_invalid_json() {
        assert_eq!(parse_github_user("not json"), None);
    }
}
