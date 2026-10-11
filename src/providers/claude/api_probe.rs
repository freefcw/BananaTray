//! Claude API Probe
//!
//! 通过 OAuth API 直接获取配额信息。
//! 无状态设计：每次 probe 从磁盘加载最新凭证，确保感知外部登录/登出变化。

use super::credentials::{refresh_oauth_token, save_credentials_atomic, ClaudeOAuthCredentials};
use super::probe::UsageProbe;
#[path = "usage_parser.rs"]
mod usage_parser;

use crate::models::QuotaInfo;
use crate::providers::common::http_client::HttpError;
use crate::providers::common::{cli, http_client};
use crate::providers::{ProviderError, ProviderResult};
use crate::utils::time_utils;
use log::debug;
use serde::Deserialize;
use std::sync::OnceLock;
use std::time::Duration;

#[cfg(test)]
use crate::models::{QuotaDetailSpec, QuotaLabelSpec, QuotaType};

/// Claude API 获取方式（无状态）
pub struct ClaudeApiProbe;

impl ClaudeApiProbe {
    /// 加载凭证并获取有效的访问令牌
    ///
    /// 如果 Token 需要刷新，会自动刷新并原子写回凭证文件
    fn get_valid_token(creds: &mut ClaudeOAuthCredentials) -> ProviderResult<String> {
        if !creds.needs_refresh() {
            return Ok(creds.access_token.clone());
        }

        let rt = creds
            .refresh_token
            .as_deref()
            .ok_or_else(|| ProviderError::session_expired(None))?;

        debug!("Claude API: token needs refresh, refreshing...");
        let response = refresh_oauth_token(rt)?;
        creds.apply_refresh(&response);
        save_credentials_atomic(creds)?;
        debug!("Claude API: token refresh succeeded");
        Ok(creds.access_token.clone())
    }

    /// 调用 Usage API
    ///
    /// 优先带 `?cedar_ember=1` 请求限额重置次数清单；服务端以
    /// 400/403 拒绝该可选参数时去掉 query 重试一次（对齐 CodexBar）。
    /// 4xx/5xx → `HttpError::HttpStatus`（由 http_client 层自动返回），
    /// 上层通过 `ProviderError::classify` 自动分类为 AuthRequired / FetchFailed 等。
    fn fetch_usage(access_token: &str) -> ProviderResult<UsageResponse> {
        const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";

        let auth_header = format!("Authorization: Bearer {}", access_token);
        let version = claude_code_version();
        let cli_user_agent = format!("User-Agent: claude-cli/{version} (external, cli)");

        let body = match http_client::get(
            &format!("{USAGE_URL}?cedar_ember=1"),
            &[
                &auth_header,
                "Accept: application/json",
                "Content-Type: application/json",
                "anthropic-beta: oauth-2025-04-20",
                &cli_user_agent,
            ],
        ) {
            Ok(body) => body,
            Err(err) if should_retry_without_reset_query(&err) => {
                let code_user_agent = format!("User-Agent: claude-code/{version}");
                http_client::get(
                    USAGE_URL,
                    &[
                        &auth_header,
                        "Accept: application/json",
                        "Content-Type: application/json",
                        "anthropic-beta: oauth-2025-04-20",
                        &code_user_agent,
                    ],
                )
                .map_err(|err| ProviderError::classify(&err))?
            }
            Err(err) => return Err(ProviderError::classify(&err)),
        };

        let usage: UsageResponse = serde_json::from_str(&body)
            .map_err(|_| ProviderError::parse_failed("Claude Usage API response"))?;

        Ok(usage)
    }

    /// 解析 Usage 响应为 QuotaInfo 列表。`now_secs` 由调用方注入，便于测试。
    fn parse_usage(usage: UsageResponse, now_secs: i64) -> Vec<QuotaInfo> {
        usage_parser::parse_usage(usage, now_secs)
    }
}

/// `?cedar_ember=1` 可选清单参数被服务端以 400/403 拒绝时，去掉 query 重试一次。
fn should_retry_without_reset_query(err: &anyhow::Error) -> bool {
    matches!(
        err.downcast_ref::<HttpError>(),
        Some(HttpError::HttpStatus { code: 400 | 403 })
    )
}

/// `claude --version` 的缓存版本号。CLI 不存在 / 输出异常时回退到
/// CodexBar 的 fallback 版本且同样缓存，不重复 spawn 进程。
fn claude_code_version() -> &'static str {
    static VERSION: OnceLock<String> = OnceLock::new();
    VERSION.get_or_init(|| {
        cli::run_command_with_timeout("claude", &["--version"], Duration::from_secs(3))
            .ok()
            .and_then(|output| parse_claude_version(&cli::stdout_text(&output)))
            .unwrap_or_else(|| "2.1.0".to_string())
    })
}

/// 解析 `claude --version` 输出（如 `"2.1.3 (Claude Code)"`）：取首个空白分隔
/// token，必须以 ASCII 数字开头，否则视为无效输出。
fn parse_claude_version(stdout: &str) -> Option<String> {
    let token = stdout.split_whitespace().next()?;
    if token.starts_with(|c: char| c.is_ascii_digit()) {
        Some(token.to_string())
    } else {
        None
    }
}

impl UsageProbe for ClaudeApiProbe {
    fn probe(&self) -> ProviderResult<Vec<QuotaInfo>> {
        let mut creds = ClaudeOAuthCredentials::load()?;
        let access_token = Self::get_valid_token(&mut creds)?;
        let usage = Self::fetch_usage(&access_token)?;
        let quotas = Self::parse_usage(usage, time_utils::now_epoch_secs());

        if quotas.is_empty() {
            return Err(ProviderError::no_data());
        }

        Ok(quotas)
    }

    fn is_available(&self) -> bool {
        ClaudeOAuthCredentials::try_load().is_some()
    }
}

// ============================================================================
// API 响应模型
// ============================================================================

#[derive(Debug, Deserialize)]
struct UsageResponse {
    five_hour: Option<UsageQuotaData>,
    seven_day: Option<UsageQuotaData>,
    seven_day_sonnet: Option<UsageQuotaData>,
    seven_day_opus: Option<UsageQuotaData>,
    extra_usage: Option<ExtraUsageData>,
    /// 云端赠送额度（普通 usage 响应即可能返回）。保持为松散 Value：
    /// 该块形态异常不允许拖垮会话 / 周窗口配额。
    iguana_necktie: Option<serde_json::Value>,
    /// 限额重置次数库存（`?cedar_ember=1` 才返回），同样松散解析。
    cedar_ember: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct UsageQuotaData {
    utilization: Option<f64>,
    resets_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ExtraUsageData {
    is_enabled: Option<bool>,
    used_credits: Option<f64>,
    monthly_limit: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // 2025-01-01T00:00:00Z
    const NOW: i64 = 1_735_689_600;

    fn usage_response(value: serde_json::Value) -> UsageResponse {
        serde_json::from_value(value).unwrap()
    }

    fn quota_with_label(usage: UsageResponse, label: QuotaLabelSpec) -> Option<QuotaInfo> {
        ClaudeApiProbe::parse_usage(usage, NOW)
            .into_iter()
            .find(|q| q.label_spec == label)
    }

    fn cloud_quota(block: serde_json::Value) -> Option<QuotaInfo> {
        let usage = usage_response(serde_json::json!({ "iguana_necktie": block }));
        quota_with_label(usage, QuotaLabelSpec::CloudCredits)
    }

    fn reset_quota(cedar_ember: serde_json::Value) -> Option<QuotaInfo> {
        let usage = usage_response(serde_json::json!({ "cedar_ember": cedar_ember }));
        quota_with_label(usage, QuotaLabelSpec::LimitResetCredits)
    }

    // ── should_retry_without_reset_query ────────────────────

    #[test]
    fn retry_without_reset_query_only_for_400_and_403() {
        for code in [400u16, 403] {
            let err: anyhow::Error = HttpError::HttpStatus { code }.into();
            assert!(should_retry_without_reset_query(&err), "HTTP {code}");
        }
        for code in [401u16, 429, 500] {
            let err: anyhow::Error = HttpError::HttpStatus { code }.into();
            assert!(!should_retry_without_reset_query(&err), "HTTP {code}");
        }
        let timeout: anyhow::Error = HttpError::Timeout.into();
        assert!(!should_retry_without_reset_query(&timeout));
        let transport: anyhow::Error = HttpError::Transport("dns failed".into()).into();
        assert!(!should_retry_without_reset_query(&transport));
    }

    // ── parse_claude_version ────────────────────────────────

    #[test]
    fn parse_claude_version_extracts_semver_token() {
        assert_eq!(
            parse_claude_version("2.1.3 (Claude Code)\n"),
            Some("2.1.3".to_string())
        );
        assert_eq!(parse_claude_version(""), None);
        assert_eq!(parse_claude_version("claude: not found"), None);
    }

    // ── parse_cloud_credits（iguana_necktie）─────────────────

    #[test]
    fn cloud_credits_reported_remaining_wins() {
        let quota = cloud_quota(serde_json::json!({
            "limit_dollars": 50.0,
            "used_dollars": 40.0,
            "remaining_dollars": 20.0,
        }))
        .expect("cloud credits");
        // 上报的 remaining=20 优先：used = 50 - 20 = 30（不采用上报的 used=40）
        assert_eq!(quota.used, 30.0);
        assert_eq!(quota.limit, 50.0);
        assert_eq!(quota.quota_type, QuotaType::Credit);
        assert_eq!(quota.stable_key, "cloud-credits");
    }

    #[test]
    fn cloud_credits_derives_remaining_from_used() {
        let quota = cloud_quota(serde_json::json!({
            "limit_dollars": 50.0,
            "used_dollars": 10.0,
        }))
        .expect("cloud credits");
        // remaining 由 50 - 10 推导 → 40；used = limit - remaining = 10
        assert_eq!(quota.used, 10.0);
        assert_eq!(quota.limit, 50.0);
    }

    #[test]
    fn cloud_credits_exhausted_still_shown() {
        let quota = cloud_quota(serde_json::json!({
            "limit_dollars": 50.0,
            "remaining_dollars": 0.0,
        }))
        .expect("exhausted credits are still shown");
        assert_eq!(quota.used, 50.0);
    }

    #[test]
    fn cloud_credits_rejects_invalid_shapes() {
        // limit 必须 > 0
        assert!(cloud_quota(serde_json::json!({
            "limit_dollars": 0.0,
            "remaining_dollars": 0.0,
        }))
        .is_none());
        // used 超出 limit
        assert!(cloud_quota(serde_json::json!({
            "limit_dollars": 10.0,
            "used_dollars": 20.0,
        }))
        .is_none());
        // used / remaining 都缺
        assert!(cloud_quota(serde_json::json!({
            "limit_dollars": 10.0,
        }))
        .is_none());
        // limit 缺失
        assert!(cloud_quota(serde_json::json!({
            "remaining_dollars": 5.0,
        }))
        .is_none());
    }

    #[test]
    fn cloud_credits_uses_valid_remaining_when_used_is_malformed() {
        let quota = cloud_quota(serde_json::json!({
            "limit_dollars": 50.0,
            "used_dollars": "not-a-number",
            "remaining_dollars": 20.0,
        }))
        .expect("valid remaining amount");
        assert_eq!(quota.used, 30.0);
    }

    #[test]
    fn cloud_credits_expired_or_unparseable_resets_at_hidden() {
        // 已到期 → 不展示
        assert!(cloud_quota(serde_json::json!({
            "limit_dollars": 50.0,
            "remaining_dollars": 10.0,
            "resets_at": "2024-12-31T00:00:00Z",
        }))
        .is_none());
        // 不可解析 → 不展示
        assert!(cloud_quota(serde_json::json!({
            "limit_dollars": 50.0,
            "remaining_dollars": 10.0,
            "resets_at": "not-a-date",
        }))
        .is_none());
    }

    #[test]
    fn cloud_credits_locked_hidden() {
        assert!(cloud_quota(serde_json::json!({
            "limit_dollars": 50.0,
            "remaining_dollars": 10.0,
            "locked_reason": "fraud_review",
        }))
        .is_none());
    }

    #[test]
    fn cloud_credits_future_expiry_becomes_days_detail() {
        // 2025-01-11T00:00:00Z = NOW + 10 天
        let quota = cloud_quota(serde_json::json!({
            "limit_dollars": 50.0,
            "remaining_dollars": 10.0,
            "resets_at": "2025-01-11T00:00:00Z",
        }))
        .expect("cloud credits");
        assert!(matches!(
            quota.detail_spec,
            Some(QuotaDetailSpec::ExpiresInDays { days: 10 })
        ));
    }

    #[test]
    fn malformed_cloud_credits_block_does_not_drop_windows() {
        let usage = usage_response(serde_json::json!({
            "five_hour": { "utilization": 25.0 },
            "seven_day": { "utilization": 60.0 },
            "iguana_necktie": "not-an-object",
        }));
        let quotas = ClaudeApiProbe::parse_usage(usage, NOW);
        assert_eq!(quotas.len(), 2);
        assert_eq!(quotas[0].quota_type, QuotaType::Session);
        assert_eq!(quotas[1].quota_type, QuotaType::Weekly);
    }

    // ── parse_limit_reset_credits（cedar_ember）───────────────

    #[test]
    fn reset_credits_sum_across_grants() {
        let quota = reset_quota(serde_json::json!({
            "eligible": true,
            "grants": [
                { "resets_left": 1, "resets_total": 1, "paused": false },
                { "resets_left": 2, "resets_total": 2, "paused": false },
            ],
        }))
        .expect("reset credits");
        assert_eq!(quota.remaining_balance, Some(3.0));
        assert_eq!(quota.quota_type, QuotaType::Count);
        assert_eq!(quota.stable_key, "limit-reset-credits");
        assert!(quota.detail_spec.is_none());
    }

    #[test]
    fn reset_credits_excludes_paused_expired_and_not_started() {
        let quota = reset_quota(serde_json::json!({
            "eligible": true,
            "grants": [
                { "resets_left": 5, "paused": true },
                { "resets_left": 5, "paused": false, "ends_at": "2024-12-31T00:00:00Z" },
                { "resets_left": 5, "paused": false, "starts_at": "2025-02-01T00:00:00Z" },
                { "resets_left": 1, "paused": false },
            ],
        }))
        .expect("only one grant counts");
        assert_eq!(quota.remaining_balance, Some(1.0));
    }

    #[test]
    fn reset_credits_drops_only_malformed_grants() {
        let quota = reset_quota(serde_json::json!({
            "eligible": true,
            "grants": [
                // 缺 paused（必填）→ 只丢这条
                { "resets_left": 9 },
                // resets_total < resets_left → 只丢这条
                { "resets_left": 9, "resets_total": 1, "paused": false },
                // 给了 ends_at 却无法解析 → 只丢这条
                { "resets_left": 9, "paused": false, "ends_at": "junk" },
                { "resets_left": 2, "paused": false },
            ],
        }))
        .expect("malformed grants dropped");
        assert_eq!(quota.remaining_balance, Some(2.0));
    }

    #[test]
    fn reset_credits_requires_eligible() {
        assert!(reset_quota(serde_json::json!({
            "eligible": false,
            "grants": [{ "resets_left": 1, "paused": false }],
        }))
        .is_none());
        // eligible 缺失同样不展示
        assert!(reset_quota(serde_json::json!({
            "grants": [{ "resets_left": 1, "paused": false }],
        }))
        .is_none());
    }

    #[test]
    fn reset_credits_rejects_implausible_sum() {
        assert!(reset_quota(serde_json::json!({
            "eligible": true,
            "grants": [
                { "resets_left": 30, "paused": false },
                { "resets_left": 30, "paused": false },
            ],
        }))
        .is_none());
    }

    #[test]
    fn reset_credits_rejects_overflowing_sum() {
        assert!(reset_quota(serde_json::json!({
            "eligible": true,
            "grants": [
                { "resets_left": 1, "paused": false },
                { "resets_left": u64::MAX, "paused": false },
            ],
        }))
        .is_none());
    }

    #[test]
    fn reset_credits_zero_available_hidden() {
        assert!(reset_quota(serde_json::json!({
            "eligible": true,
            "grants": [{ "resets_left": 0, "paused": false }],
        }))
        .is_none());
    }

    #[test]
    fn reset_credits_next_expiry_is_earliest() {
        let quota = reset_quota(serde_json::json!({
            "eligible": true,
            "grants": [
                { "resets_left": 1, "paused": false, "ends_at": "2025-01-10T00:00:00Z" },
                { "resets_left": 1, "paused": false, "ends_at": "2025-01-05T00:00:00Z" },
                { "resets_left": 1, "paused": false },
            ],
        }))
        .expect("reset credits");
        assert_eq!(quota.remaining_balance, Some(3.0));
        // 2025-01-05T00:00:00Z = NOW + 4 天
        assert!(matches!(
            quota.detail_spec,
            Some(QuotaDetailSpec::ExpiresInDays { days: 4 })
        ));
    }
}
