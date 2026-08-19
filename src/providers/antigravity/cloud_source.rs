//! Antigravity 云端 quota source：读取 agy CLI 的 Keychain token，
//! 调用 Google 内部 quota summary API 获取账户额度。
//!
//! BananaTray 不持有 refresh token，也不直接写 Keychain；access token 过期时
//! 通过有界的 `agy models` 调用触发 CLI 续期，续期后仍无有效 token 才返回
//! `SessionExpired`。

use crate::models::{QuotaDetailSpec, QuotaInfo, QuotaLabelSpec, QuotaType, RefreshData};
use crate::providers::common::cli;
use crate::providers::ProviderError;
use crate::utils::time_utils::{is_expired_epoch_secs, parse_iso8601_to_epoch};
use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use log::{debug, info, warn};
use serde::Deserialize;
use std::process::Output;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub(crate) const CLOUD_API_SOURCE_LABEL: &str = "antigravity cloud";

const QUOTA_API_BASE: &str = "https://daily-cloudcode-pa.googleapis.com";
const LOAD_CODE_ASSIST_PATH: &str = "/v1internal:loadCodeAssist";
const RETRIEVE_USER_QUOTA_SUMMARY_PATH: &str = "/v1internal:retrieveUserQuotaSummary";

/// 近期 agy 请求和公开远程实现都携带 Antigravity User-Agent；部分复现中
/// 省略该标识会触发 429，因此固定发送以保持客户端兼容性。
const USER_AGENT: &str = "User-Agent: antigravity";

/// macOS Keychain 中 agy CLI 凭证的存储位置（go-keyring 约定）。
#[cfg(target_os = "macos")]
const KEYCHAIN_SERVICE: &str = "gemini";
#[cfg(target_os = "macos")]
const KEYCHAIN_ACCOUNT: &str = "antigravity";

/// go-keyring 在 macOS 上的存储值带此前缀，后面跟 base64(JSON)。
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const KEYRING_BASE64_PREFIX: &str = "go-keyring-base64:";

/// 撞到 RESOURCE_EXHAUSTED（429）后的本地冷却窗口。
///
/// 该端点对非 agy 客户端的调用限流非常激进，冷却期内直接跳过云端源，
/// 走 live / cache fallback，避免每个刷新周期都触发一次注定失败的请求。
const RATE_LIMIT_COOLDOWN: Duration = Duration::from_secs(30 * 60);
#[cfg(target_os = "macos")]
const KEYCHAIN_COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
const AGY_REFRESH_TIMEOUT: Duration = Duration::from_secs(15);

/// 429 冷却截止时间；None 表示当前不在冷却期。
static RATE_LIMIT_UNTIL: Mutex<Option<Instant>> = Mutex::new(None);

// ── token 层 ──────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct AgyKeychainPayload {
    #[serde(default)]
    token: Option<AgyToken>,
}

#[derive(Debug, Deserialize)]
struct AgyToken {
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    expiry: Option<String>,
}

/// 从 macOS Keychain 读取 agy CLI 的完整凭证 payload；读取失败视为云端源不可用。
#[cfg(target_os = "macos")]
fn read_agy_keychain_payload() -> Option<AgyKeychainPayload> {
    read_agy_keychain_payload_with(cli::run_command_with_timeout)
}

#[cfg(target_os = "macos")]
fn read_agy_keychain_payload_with(
    run: impl FnOnce(&str, &[&str], Duration) -> Result<Output>,
) -> Option<AgyKeychainPayload> {
    let output = run(
        "/usr/bin/security",
        &[
            "find-generic-password",
            "-s",
            KEYCHAIN_SERVICE,
            "-a",
            KEYCHAIN_ACCOUNT,
            "-w",
        ],
        KEYCHAIN_COMMAND_TIMEOUT,
    )
    .ok()?;

    if !output.status.success() {
        debug!(
            target: "providers",
            "antigravity: macOS Keychain lookup failed (no agy entry)"
        );
        return None;
    }

    let raw = String::from_utf8(output.stdout).ok()?;
    parse_keychain_payload(raw.trim())
        .inspect_err(
            |err| debug!(target: "providers", "antigravity: agy Keychain payload invalid: {}", err),
        )
        .ok()
}

/// 非 macOS 平台不支持 Keychain 读取，云端源直接不可用。
#[cfg(not(target_os = "macos"))]
fn read_agy_keychain_payload() -> Option<AgyKeychainPayload> {
    None
}

/// 云端源是否已配置：Keychain 中存在非空 access token。
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn has_keychain_token() -> bool {
    extract_access_token(read_agy_keychain_payload()).is_some()
}

fn extract_access_token(payload: Option<AgyKeychainPayload>) -> Option<String> {
    let token = payload?.token?;
    let access_token = token.access_token?;
    (!access_token.is_empty()).then_some(access_token)
}

/// 解析 go-keyring 存储值：`go-keyring-base64:` 前缀 + base64(JSON payload)。
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn parse_keychain_payload(raw: &str) -> Result<AgyKeychainPayload> {
    let encoded = raw
        .strip_prefix(KEYRING_BASE64_PREFIX)
        .ok_or_else(|| anyhow::anyhow!("unexpected keychain payload format"))?;
    let decoded = STANDARD
        .decode(encoded)
        .with_context(|| "keychain payload is not valid base64")?;
    serde_json::from_slice(&decoded).with_context(|| "keychain payload is not valid JSON")
}

/// 检查 token expiry（RFC3339）是否已过期。
fn is_token_expired(payload_expiry: Option<&str>) -> bool {
    payload_expiry
        .and_then(parse_iso8601_to_epoch)
        .map(|expiry| is_expired_epoch_secs(expiry as f64))
        .unwrap_or(false)
}

/// payload 里的 token 是否已过期；缺 expiry 时宽容处理（当作未过期）。
fn payload_token_expired(payload: Option<&AgyKeychainPayload>) -> bool {
    is_token_expired(
        payload
            .and_then(|p| p.token.as_ref())
            .and_then(|token| token.expiry.as_deref()),
    )
}

fn resolve_access_token_with_refresh(
    mut payload: AgyKeychainPayload,
    refresh: impl FnOnce(),
    reread: impl FnOnce() -> Option<AgyKeychainPayload>,
) -> Result<String> {
    if payload_token_expired(Some(&payload)) {
        refresh();
        payload = reread().ok_or_else(|| anyhow::Error::new(session_expired_error()))?;
    }

    if payload_token_expired(Some(&payload)) {
        return Err(session_expired_error().into());
    }

    extract_access_token(Some(payload)).ok_or_else(|| session_expired_error().into())
}

fn session_expired_error() -> ProviderError {
    ProviderError::session_expired(Some(crate::models::FailureAdvice::LoginCli {
        cli: "agy".to_string(),
    }))
}

/// 跑一次非交互 agy 命令，让 CLI 用自持的 refresh_token 自动刷新 Keychain。
///
/// agy 的 access_token 有效期约 1 小时，过期是常态而不是需要重新登录；
/// CLI 侧任何一次联网调用都会顺带完成续期。这里只负责触发，
/// 刷新是否成功由调用方重读 Keychain 判定。
fn try_refresh_token_via_agy() {
    try_refresh_token_via_agy_with(cli::run_command_with_timeout);
}

fn try_refresh_token_via_agy_with(run: impl FnOnce(&str, &[&str], Duration) -> Result<Output>) {
    info!(
        target: "providers",
        "antigravity: token expired, triggering agy CLI token refresh"
    );
    let output = run("agy", &["models"], AGY_REFRESH_TIMEOUT);
    match output {
        Ok(status) if !status.status.success() => {
            warn!(
                target: "providers",
                "antigravity: agy CLI token refresh exited with {:?}", status.status
            );
        }
        Err(err) => {
            warn!(
                target: "providers",
                "antigravity: failed to run agy CLI for token refresh: {err}"
            );
        }
        _ => {}
    }
}

// ── 429 冷却层 ────────────────────────────────────────

/// 冷却状态机的纯判断：给定当前时间与截止时间，决定是否跳过云端源。
fn in_cooldown(now: Instant, until: Option<Instant>) -> bool {
    until.is_some_and(|deadline| now < deadline)
}

fn rate_limit_active() -> bool {
    let guard = RATE_LIMIT_UNTIL
        .lock()
        .expect("antigravity rate limit state lock poisoned");
    in_cooldown(Instant::now(), *guard)
}

fn mark_rate_limited(now: Instant) {
    let mut guard = RATE_LIMIT_UNTIL
        .lock()
        .expect("antigravity rate limit state lock poisoned");
    *guard = Some(now + RATE_LIMIT_COOLDOWN);
}

fn clear_rate_limit() {
    let mut guard = RATE_LIMIT_UNTIL
        .lock()
        .expect("antigravity rate limit state lock poisoned");
    *guard = None;
}

// ── API 层 ────────────────────────────────────────────

/// 云端响应的 bucket；Google JSON transcoding 主形态是 camelCase，
/// 字段同时兼容 snake_case 以防上游序列化形态变化。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuotaSummaryBucket {
    #[serde(default, alias = "bucket_id")]
    bucket_id: Option<String>,
    #[serde(default, alias = "display_name")]
    display_name: Option<String>,
    #[serde(default, alias = "window")]
    window: Option<String>,
    #[serde(default, alias = "remaining_fraction")]
    remaining_fraction: Option<f64>,
    #[serde(default, alias = "reset_time")]
    reset_time: Option<String>,
}

/// 实际响应把 bucket 按模型组归类（Gemini 组 / Claude+GPT 组），各组共享
/// weekly + 5h 两个窗口；顶层 `buckets` 字段实测未出现，但保留兼容。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuotaSummaryResponse {
    #[serde(default)]
    buckets: Vec<QuotaSummaryBucket>,
    #[serde(default)]
    groups: Vec<QuotaSummaryGroup>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuotaSummaryGroup {
    #[serde(default)]
    buckets: Vec<QuotaSummaryBucket>,
    /// 组名，如 "Gemini Models" / "Claude and GPT models"
    #[serde(default, alias = "display_name")]
    display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoadCodeAssistResponse {
    cloudaicompanion_project: Option<String>,
}

pub fn fetch_refresh_data() -> Result<RefreshData> {
    if rate_limit_active() {
        debug!(
            target: "providers",
            "antigravity: cloud API in rate-limit cooldown, skipping"
        );
        return Err(ProviderError::unavailable("cloud API in rate-limit cooldown").into());
    }

    let payload = read_agy_keychain_payload().ok_or_else(|| {
        // Keychain 没有 agy 凭证：说明用户没装 / 没登录过 agy CLI，
        // 属于正常的不可用而不是错误，静默走本地源。
        ProviderError::unavailable("agy Keychain token not found")
    })?;

    // access_token 约 1 小时过期，过期是常态；跑一次非交互 agy 让 CLI
    // 用自持的 refresh_token 续期（实测 `agy models` 即可触发），再重读 Keychain。
    // 续期失败或用户没装 CLI 时才报 SessionExpired 引导重新登录。
    let access_token = resolve_access_token_with_refresh(
        payload,
        try_refresh_token_via_agy,
        read_agy_keychain_payload,
    )?;

    info!(
        target: "providers",
        "antigravity: fetching quota summary from cloud API"
    );

    let response_text = fetch_quota_summary_response(
        &access_token,
        crate::providers::common::http_client::post_json,
    )
    .map_err(|err| {
        if is_rate_limit_error(&err) {
            mark_rate_limited(Instant::now());
            warn!(
                target: "providers",
                "antigravity: cloud API rate limited (429), cooling down for 30 minutes"
            );
            ProviderError::fetch_failed("cloud API rate limited")
        } else {
            ProviderError::classify(&err)
        }
    })?;

    clear_rate_limit();

    let response: QuotaSummaryResponse = serde_json::from_str(&response_text)
        .with_context(|| "Failed to parse cloud quota summary response")?;

    build_refresh_data(response)
}

fn fetch_quota_summary_response(
    access_token: &str,
    mut post_json: impl FnMut(&str, &[&str], &str) -> Result<String>,
) -> Result<String> {
    let auth_header = format!("Authorization: Bearer {access_token}");
    let headers = [auth_header.as_str(), USER_AGENT];
    let load_url = format!("{QUOTA_API_BASE}{LOAD_CODE_ASSIST_PATH}");
    let load_body = serde_json::json!({
        "metadata": {
            "ideType": "ANTIGRAVITY",
            "pluginType": "GEMINI"
        }
    })
    .to_string();
    let project = match post_json(&load_url, &headers, &load_body) {
        Ok(load_response) => match serde_json::from_str::<LoadCodeAssistResponse>(&load_response) {
            Ok(response) => response
                .cloudaicompanion_project
                .filter(|project| !project.is_empty()),
            Err(err) => {
                debug!(
                    target: "providers",
                    "antigravity: loadCodeAssist response could not be parsed: {err}"
                );
                None
            }
        },
        Err(err) => {
            debug!(
                target: "providers",
                "antigravity: project discovery failed, trying unscoped quota request: {err}"
            );
            None
        }
    };

    let summary_url = format!("{QUOTA_API_BASE}{RETRIEVE_USER_QUOTA_SUMMARY_PATH}");
    let body = project
        .map(|project| serde_json::json!({ "project": project }).to_string())
        .unwrap_or_else(|| "{}".to_string());
    post_json(&summary_url, &headers, &body)
}

/// `HttpError::HttpStatus { code: 429 }` 检测（正文被 http 层有意丢弃，只看状态码）。
fn is_rate_limit_error(err: &anyhow::Error) -> bool {
    use crate::providers::common::http_client::HttpError;

    matches!(
        err.downcast_ref::<HttpError>(),
        Some(HttpError::HttpStatus { code: 429 })
    )
}

fn build_refresh_data(response: QuotaSummaryResponse) -> Result<RefreshData> {
    // 实测响应只有 groups（每组共享 weekly + 5h 两个 bucket）；
    // 顶层 buckets 保留为兼容形状，groups 缺失时兜底。
    let mut quotas: Vec<QuotaInfo> = response
        .groups
        .iter()
        .flat_map(|group| {
            group
                .buckets
                .iter()
                .filter_map(|bucket| build_bucket_quota(bucket, group.display_name.as_deref()))
        })
        .collect();

    if quotas.is_empty() {
        quotas = response
            .buckets
            .iter()
            .filter_map(|bucket| build_bucket_quota(bucket, None))
            .collect();
    }

    if quotas.is_empty() {
        return Err(ProviderError::no_data().into());
    }

    Ok(RefreshData::with_account(quotas, None, None).with_source_label(CLOUD_API_SOURCE_LABEL))
}

/// 把单个 bucket 转成 QuotaInfo；无法解释的 bucket（缺 id / 缺剩余量）跳过。
/// stable_key 带 group 前缀避免两组同 window 的 bucket 互相覆盖显示状态。
/// label 走结构化语义（如 "Weekly (Gemini)"），不透传上游长文案，
/// 避免挤爆托盘弹窗的右对齐状态区。
fn build_bucket_quota(bucket: &QuotaSummaryBucket, group: Option<&str>) -> Option<QuotaInfo> {
    let bucket_id = bucket.bucket_id.as_deref()?;
    let group_name = group.and_then(short_group_name);

    let stable_key = match &group_name {
        Some(group) => format!("{group}:{bucket_id}"),
        None => bucket_id.to_string(),
    };

    let reset_detail = bucket.reset_time.as_deref().and_then(parse_reset_time);

    // window 字段（如 "weekly" / "5h"）决定标题语义，缺省退回 Raw 展示。
    let quota_type = classify_window(bucket.window.as_deref());
    let label = match (bucket.window.as_deref(), group_name) {
        (Some(window), Some(group)) => window_label(window, &group),
        (Some(window), None) => window_label_plain(window),
        _ => QuotaLabelSpec::Raw(
            bucket
                .display_name
                .clone()
                .unwrap_or_else(|| bucket_id.to_string()),
        ),
    };

    Some(QuotaInfo::with_key_from_remaining_fraction(
        stable_key,
        label,
        bucket.remaining_fraction?,
        quota_type,
        reset_detail,
    ))
}

/// 组名压缩成短标识：只保留 "Gemini Models" → "Gemini" 这类首个词，
/// "Claude and GPT models" → "Claude/GPT" 这类专名列表。
fn short_group_name(group: &str) -> Option<String> {
    let lowered = group.to_ascii_lowercase();
    if lowered.contains("gemini") {
        Some("Gemini".to_string())
    } else if lowered.contains("claude") {
        Some("Claude/GPT".to_string())
    } else {
        // 未知组：取首个词，仍比全名短
        group
            .split_whitespace()
            .next()
            .map(str::to_string)
            .filter(|s| !s.is_empty())
    }
}

/// window + group → "Weekly (Gemini)" / "Session (Gemini)" 形态的标题语义。
fn window_label(window: &str, group: &str) -> QuotaLabelSpec {
    match window.to_ascii_uppercase().as_str() {
        "WEEKLY" | "WEEK" => QuotaLabelSpec::WeeklyModel {
            model: group.to_string(),
        },
        "5H" => QuotaLabelSpec::ModelSpecificSession {
            model: group.to_string(),
        },
        _ => QuotaLabelSpec::Raw(format!("{window} ({group})")),
    }
}

/// 无 group 时的标题：直接用周期语义（"周配额" / "会话"）。
fn window_label_plain(window: &str) -> QuotaLabelSpec {
    match window.to_ascii_uppercase().as_str() {
        "WEEKLY" | "WEEK" => QuotaLabelSpec::Weekly,
        "5H" => QuotaLabelSpec::Session,
        _ => QuotaLabelSpec::Raw(window.to_string()),
    }
}

/// Google.protobuf.Timestamp 的 JSON 形态是 RFC3339 字符串。
fn parse_reset_time(reset_time: &str) -> Option<QuotaDetailSpec> {
    parse_iso8601_to_epoch(reset_time).map(|epoch_secs| QuotaDetailSpec::ResetAt { epoch_secs })
}

/// window 实测取值：`weekly` / `5h`；DAY / MONTH 保留映射，未知窗口按 General。
fn classify_window(window: Option<&str>) -> QuotaType {
    match window.map(str::to_ascii_uppercase).as_deref() {
        Some("WEEKLY") | Some("WEEK") => QuotaType::Weekly,
        Some("5H") => QuotaType::Session,
        Some("DAY") => QuotaType::General,
        Some("MONTH") => QuotaType::Monthly,
        _ => QuotaType::General,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_keychain_payload_decodes_go_keyring_base64() {
        let json = r#"{"token":{"access_token":"abc123","token_type":"Bearer","refresh_token":"r","expiry":"2026-08-15T11:29:31.761968+08:00"},"auth_method":"consumer"}"#;
        let encoded = STANDARD.encode(json);
        let raw = format!("{}{}", KEYRING_BASE64_PREFIX, encoded);

        let payload = parse_keychain_payload(&raw).unwrap();
        assert_eq!(
            payload.token.unwrap().access_token.as_deref(),
            Some("abc123")
        );
    }

    #[test]
    fn parse_keychain_payload_rejects_missing_prefix() {
        assert!(parse_keychain_payload("plain-text").is_err());
    }

    #[test]
    fn parse_keychain_payload_rejects_invalid_base64() {
        let raw = format!("{}!!!", KEYRING_BASE64_PREFIX);
        assert!(parse_keychain_payload(&raw).is_err());
    }

    #[test]
    fn parse_keychain_payload_rejects_invalid_json() {
        let raw = format!("{}{}", KEYRING_BASE64_PREFIX, STANDARD.encode("not json"));
        assert!(parse_keychain_payload(&raw).is_err());
    }

    #[test]
    fn parse_keychain_payload_tolerates_missing_token() {
        let raw = format!("{}{}", KEYRING_BASE64_PREFIX, STANDARD.encode("{}"));
        let payload = parse_keychain_payload(&raw).unwrap();
        assert!(payload.token.is_none());
    }

    #[test]
    fn token_expiry_detects_past_and_future() {
        assert!(is_token_expired(Some("2020-01-01T00:00:00Z")));
        assert!(!is_token_expired(Some("2099-01-01T00:00:00Z")));
        assert!(!is_token_expired(Some("not-a-date")));
        assert!(!is_token_expired(None));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn keychain_lookup_uses_bounded_command_runner() {
        let mut observed = None;
        let payload = read_agy_keychain_payload_with(|binary, args, timeout| {
            observed = Some((
                binary.to_string(),
                args.iter()
                    .map(|arg| (*arg).to_string())
                    .collect::<Vec<_>>(),
                timeout,
            ));
            Err(anyhow::anyhow!("stop after observing command"))
        });

        assert!(payload.is_none());
        let (binary, args, timeout) = observed.unwrap();
        assert_eq!(binary, "/usr/bin/security");
        assert_eq!(args[0], "find-generic-password");
        assert_eq!(timeout, Duration::from_secs(5));
    }

    #[test]
    fn agy_refresh_uses_bounded_command_runner() {
        let mut observed = None;
        try_refresh_token_via_agy_with(|binary, args, timeout| {
            observed = Some((
                binary.to_string(),
                args.iter()
                    .map(|arg| (*arg).to_string())
                    .collect::<Vec<_>>(),
                timeout,
            ));
            Err(anyhow::anyhow!("stop after observing command"))
        });

        let (binary, args, timeout) = observed.unwrap();
        assert_eq!(binary, "agy");
        assert_eq!(args, ["models"]);
        assert_eq!(timeout, Duration::from_secs(15));
    }

    #[test]
    fn expired_token_still_expired_after_cli_refresh_returns_session_expired() {
        let expired_payload = || AgyKeychainPayload {
            token: Some(AgyToken {
                access_token: Some("stale-token".to_string()),
                expiry: Some("2020-01-01T00:00:00Z".to_string()),
            }),
        };

        let err =
            resolve_access_token_with_refresh(expired_payload(), || {}, || Some(expired_payload()))
                .unwrap_err();

        assert!(matches!(
            err.downcast_ref::<ProviderError>(),
            Some(ProviderError::SessionExpired { .. })
        ));
    }

    #[test]
    fn expired_token_refreshed_by_cli_returns_new_access_token() {
        let expired_payload = AgyKeychainPayload {
            token: Some(AgyToken {
                access_token: Some("stale-token".to_string()),
                expiry: Some("2020-01-01T00:00:00Z".to_string()),
            }),
        };
        let refreshed_payload = AgyKeychainPayload {
            token: Some(AgyToken {
                access_token: Some("fresh-token".to_string()),
                expiry: Some("2099-01-01T00:00:00Z".to_string()),
            }),
        };

        let token =
            resolve_access_token_with_refresh(expired_payload, || {}, || Some(refreshed_payload))
                .unwrap();

        assert_eq!(token, "fresh-token");
    }

    #[test]
    fn parse_quota_summary_camel_case() {
        // 实测形态：bucket 全部在 groups 里，每组共享 weekly + 5h 两个窗口
        let body = r#"{
            "groups": [
                {
                    "displayName": "Gemini Models",
                    "buckets": [
                        {
                            "bucketId": "gemini-weekly",
                            "displayName": "Weekly Limit Remaining",
                            "window": "weekly",
                            "resetTime": "2026-08-19T17:17:06Z",
                            "remainingFraction": 0.98
                        },
                        {
                            "bucketId": "gemini-5h",
                            "displayName": "Five Hour Limit Remaining",
                            "window": "5h",
                            "resetTime": "2026-08-15T20:33:37Z",
                            "remainingFraction": 0.75
                        }
                    ]
                }
            ]
        }"#;
        let response: QuotaSummaryResponse = serde_json::from_str(body).unwrap();
        let data = build_refresh_data(response).unwrap();

        assert_eq!(data.source_label, Some(CLOUD_API_SOURCE_LABEL.to_string()));
        assert_eq!(data.quotas.len(), 2);
        let weekly = &data.quotas[0];
        assert_eq!(weekly.stable_key, "Gemini:gemini-weekly");
        assert_eq!(weekly.quota_type, QuotaType::Weekly);
        assert!(matches!(
            weekly.label_spec,
            QuotaLabelSpec::WeeklyModel { .. }
        ));
        assert!((weekly.used - 2.0).abs() < 0.01);
        let session = &data.quotas[1];
        assert_eq!(session.stable_key, "Gemini:gemini-5h");
        assert_eq!(session.quota_type, QuotaType::Session);
        assert!(matches!(
            session.label_spec,
            QuotaLabelSpec::ModelSpecificSession { .. }
        ));
        // remaining fraction 0.75 → used 25%
        assert!((session.used - 25.0).abs() < 0.01);
        assert!(matches!(
            session.detail_spec,
            Some(QuotaDetailSpec::ResetAt { .. })
        ));
    }

    #[test]
    fn parse_quota_summary_claude_group_short_name() {
        // 用户实际遇到的场景：Claude and GPT 组的长文案标题挤爆 UI，
        // 压缩为 "Claude/GPT" 短名并走结构化标签
        let body = r#"{
            "groups": [
                {
                    "displayName": "Claude and GPT models",
                    "buckets": [
                        {
                            "bucketId": "3p-weekly",
                            "displayName": "Weekly Limit Remaining",
                            "window": "weekly",
                            "remainingFraction": 0.4
                        },
                        {
                            "bucketId": "3p-5h",
                            "displayName": "Five Hour Limit Remaining",
                            "window": "5h",
                            "remainingFraction": 0.9
                        }
                    ]
                }
            ]
        }"#;
        let response: QuotaSummaryResponse = serde_json::from_str(body).unwrap();
        let data = build_refresh_data(response).unwrap();

        let weekly = &data.quotas[0];
        assert_eq!(weekly.stable_key, "Claude/GPT:3p-weekly");
        assert!(matches!(
            weekly.label_spec,
            QuotaLabelSpec::WeeklyModel { ref model } if model == "Claude/GPT"
        ));
        let session = &data.quotas[1];
        assert_eq!(session.stable_key, "Claude/GPT:3p-5h");
        assert!(matches!(
            session.label_spec,
            QuotaLabelSpec::ModelSpecificSession { ref model } if model == "Claude/GPT"
        ));
    }

    #[test]
    fn parse_quota_summary_snake_case_top_level_buckets_fallback() {
        // groups 缺失时退回顶层 buckets（兼容形状，group 前缀为空）
        let body = r#"{
            "buckets": [
                {
                    "bucket_id": "weekly",
                    "display_name": "Weekly quota",
                    "window": "WEEK",
                    "remaining_fraction": 0.5
                }
            ]
        }"#;
        let response: QuotaSummaryResponse = serde_json::from_str(body).unwrap();
        let data = build_refresh_data(response).unwrap();

        let quota = &data.quotas[0];
        assert_eq!(quota.stable_key, "weekly");
        assert_eq!(quota.quota_type, QuotaType::Weekly);
        assert!((quota.used - 50.0).abs() < 0.01);
    }

    #[test]
    fn parse_quota_summary_skips_unusable_buckets() {
        let body = r#"{
            "groups": [
                {
                    "displayName": "G",
                    "buckets": [
                        {"bucketId": "no-fraction"},
                        {"bucketId": "disabled-bucket", "remainingFraction": 0.1, "disabled": true}
                    ]
                }
            ]
        }"#;
        let response: QuotaSummaryResponse = serde_json::from_str(body).unwrap();
        let data = build_refresh_data(response).unwrap();

        // 缺 remaining_fraction 的 bucket 被跳过；disabled 的仍保留（由用户自己隐藏）
        assert_eq!(data.quotas.len(), 1);
        assert_eq!(data.quotas[0].stable_key, "G:disabled-bucket");
    }

    #[test]
    fn parse_quota_summary_empty_buckets_is_no_data() {
        let response: QuotaSummaryResponse = serde_json::from_str("{}").unwrap();
        let err = build_refresh_data(response).unwrap_err();
        assert!(matches!(
            err.downcast_ref::<ProviderError>(),
            Some(ProviderError::NoData)
        ));
    }

    #[test]
    fn rate_limit_cooldown_state_machine() {
        let now = Instant::now();

        // 无冷却
        assert!(!in_cooldown(now, None));
        // 截止时间已过 → 不在冷却
        assert!(!in_cooldown(now, Some(now - Duration::from_secs(1))));
        // 截止时间在未来 → 冷却中
        assert!(in_cooldown(now, Some(now + Duration::from_secs(1))));
        // 恰好到达截止时间 → 冷却结束
        assert!(!in_cooldown(now, Some(now)));
    }

    #[test]
    fn classify_window_maps_known_windows() {
        assert_eq!(classify_window(Some("weekly")), QuotaType::Weekly);
        assert_eq!(classify_window(Some("5h")), QuotaType::Session);
        assert_eq!(classify_window(Some("DAY")), QuotaType::General);
        assert_eq!(classify_window(Some("MONTH")), QuotaType::Monthly);
        assert_eq!(classify_window(None), QuotaType::General);
        assert_eq!(classify_window(Some("ROLLING")), QuotaType::General);
    }

    #[test]
    fn quota_request_discovers_project_before_fetching_summary() {
        let mut calls = Vec::new();
        let response = fetch_quota_summary_response("test-token", |url, headers, body| {
            calls.push((
                url.to_string(),
                headers
                    .iter()
                    .map(|header| (*header).to_string())
                    .collect::<Vec<_>>(),
                body.to_string(),
            ));
            if url.ends_with("/v1internal:loadCodeAssist") {
                Ok(r#"{"cloudaicompanionProject":"project-123"}"#.to_string())
            } else {
                Ok(r#"{"groups":[]}"#.to_string())
            }
        })
        .unwrap();

        assert_eq!(response, r#"{"groups":[]}"#);
        assert_eq!(calls.len(), 2);
        assert!(calls[0].0.ends_with("/v1internal:loadCodeAssist"));
        assert!(calls[1].0.ends_with("/v1internal:retrieveUserQuotaSummary"));
        assert!(calls[0].1.iter().any(|header| header == USER_AGENT));
        assert!(calls[0]
            .1
            .iter()
            .any(|header| header == "Authorization: Bearer test-token"));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&calls[0].2).unwrap(),
            serde_json::json!({
                "metadata": {
                    "ideType": "ANTIGRAVITY",
                    "pluginType": "GEMINI"
                }
            })
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&calls[1].2).unwrap(),
            serde_json::json!({ "project": "project-123" })
        );
    }

    #[test]
    fn quota_request_falls_back_to_unscoped_summary_when_project_discovery_fails() {
        let mut calls = Vec::new();
        let response = fetch_quota_summary_response("test-token", |url, _headers, body| {
            calls.push((url.to_string(), body.to_string()));
            if url.ends_with("/v1internal:loadCodeAssist") {
                Err(anyhow::anyhow!("load unavailable"))
            } else {
                Ok(r#"{"groups":[]}"#.to_string())
            }
        })
        .unwrap();

        assert_eq!(response, r#"{"groups":[]}"#);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[1].1, "{}");
    }
}
