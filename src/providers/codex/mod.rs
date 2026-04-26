mod auth;
mod client;
mod config;
mod parser;
mod status_probe;

use super::{AiProvider, ProviderError, ProviderResult};
use crate::models::{
    FailureAdvice, ProviderDescriptor, ProviderKind, ProviderMetadata, RefreshData,
};
use crate::providers::common::http_client::HttpError;
use anyhow::Result;
use async_trait::async_trait;
use std::borrow::Cow;

use auth::{
    auth_path, ensure_access_token, load_credentials, refresh_access_token, resolve_account,
    CodexCredentials,
};
use client::call_usage_api;
use parser::{parse_usage_response, ParsedUsage};

super::define_unit_provider!(CodexProvider);

fn is_auth_api_error(err: &anyhow::Error) -> bool {
    err.downcast_ref::<HttpError>()
        .is_some_and(|http_error| http_error.is_auth_error())
}

fn auth_refresh_failed_error(
    usage_error: &anyhow::Error,
    refresh_error: &ProviderError,
) -> ProviderError {
    log::warn!(
        target: "providers",
        "Codex usage API returned auth error: {usage_error}"
    );
    log::warn!(
        target: "providers",
        "Codex token refresh failed: {refresh_error}"
    );
    ProviderError::session_expired(Some(FailureAdvice::ReloginCli {
        cli: "codex".to_string(),
    }))
}

#[async_trait]
impl AiProvider for CodexProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: Cow::Borrowed("codex:api"),
            metadata: ProviderMetadata {
                kind: ProviderKind::Codex,
                display_name: "Codex".into(),
                brand_name: "OpenAI".into(),
                icon_asset: "src/icons/provider-codex.svg".into(),
                dashboard_url: "https://chatgpt.com/codex/cloud/settings/analytics".into(),
                account_hint: "OpenAI account".into(),
                source_label: "openai api".into(),
            },
        }
    }

    async fn check_availability(&self) -> ProviderResult<()> {
        if auth_path().exists() {
            Ok(())
        } else {
            Err(ProviderError::config_missing("~/.codex/auth.json"))
        }
    }

    async fn refresh(&self) -> ProviderResult<RefreshData> {
        let mut credentials = load_credentials()?;
        // Proactive refresh：成功会原地 reload credentials，使后续 resolve_account
        // 与被动 refresh 路径都拿到新 id_token / refresh_token。
        ensure_access_token(&mut credentials);

        // 读一次 ~/.codex/config.toml（默认 chatgpt.com/backend-api，支持自托管网关）。
        // 同一次 refresh 内不重复读取：fallback 重试复用同一 URL。
        let usage_url = config::resolve_usage_url();
        let parsed = obtain_parsed_usage(&mut credentials, &usage_url)?;

        // account 必须在最后一次 credentials 更新之后计算，
        // 这样多账号场景下 email / plan / account_id 跟请求使用的是同一份身份。
        let account = resolve_account(&credentials);
        // 响应中的 plan_type 优先于 JWT fallback，与 CodexBar `resolvePlan` 一致。
        let account_tier = parsed.plan_type.or(account.plan);
        Ok(RefreshData::with_account(
            parsed.quotas,
            account.email,
            account_tier,
        ))
    }
}

/// OAuth 调用 + CLI 兑底的统一获取入口。
///
/// OAuth 失败且判定为可恢复时才调用 [`status_probe::fetch_via_cli`]；
/// CLI 也失败时优先返回原始 OAuth 错误（诊断价值更高）。
fn obtain_parsed_usage(
    credentials: &mut CodexCredentials,
    usage_url: &str,
) -> ProviderResult<ParsedUsage> {
    match fetch_usage(credentials, usage_url) {
        Ok(raw) => parse_usage_response(&raw),
        Err(oauth_err) if should_fallback_to_cli(&oauth_err) => {
            log::info!(
                target: "providers",
                "Codex OAuth path failed ({oauth_err}); falling back to `codex /status`"
            );
            match status_probe::fetch_via_cli() {
                Ok(parsed) => Ok(parsed),
                Err(cli_err) => {
                    log::warn!(
                        target: "providers",
                        "Codex CLI fallback also failed: {cli_err}"
                    );
                    Err(ProviderError::classify(&oauth_err))
                }
            }
        }
        Err(e) => Err(ProviderError::classify(&e)),
    }
}

/// 调用 usage API 一次；遇到 401/403 时被动刷新 token、reload credentials 并重试一次。
///
/// 重试前 reload 是关键：若 OAuth 服务端轮转了 refresh_token，
/// 而我们仍用内存里的旧值重试，可恢复的状态会被误报成 SessionExpired。
/// 同时 reload 也让 `ChatGPT-Account-Id` 与新 id_token 中的 `chatgpt_account_id` 同步。
fn fetch_usage(credentials: &mut CodexCredentials, url: &str) -> Result<String> {
    match attempt_usage(credentials, url) {
        Ok(raw) => Ok(raw),
        Err(e) if is_auth_api_error(&e) => {
            if let Err(refresh_err) = refresh_access_token(&credentials.refresh_token) {
                return Err(auth_refresh_failed_error(&e, &refresh_err).into());
            }
            // save_refreshed_tokens 已将最新 token 写盘；reload 拿到 up-to-date 值。
            // reload 失败则维持旧值强行重试——总比抛给上层好。
            if let Ok(reloaded) = load_credentials() {
                *credentials = reloaded;
            }
            attempt_usage(credentials, url)
        }
        Err(e) => Err(e),
    }
}

/// 用当前 credentials 发起一次 usage API 调用。
fn attempt_usage(credentials: &CodexCredentials, url: &str) -> Result<String> {
    let account = resolve_account(credentials);
    call_usage_api(
        url,
        &credentials.access_token,
        account.account_id.as_deref(),
    )
}

/// 判定 OAuth 失败是否值得兑底到 CLI。
///
/// 优先看 HTTP 状态码做精确区分；codex CLI 进程内部仍会调用同一 ChatGPT API，
/// 所以举凡“身份 / 限流”这类随调用者走的错误都不应该兑底。
///
/// 决策表：
/// - **HTTP Timeout / Transport 错误** → ✅ 网络 / 连接问题，CLI 走同一后端但可能走不同路由
/// - **HTTP 5xx** → ✅ 服务端临时故障，CLI 重试可能接到其他实例
/// - **HTTP 429** → ❌ 限流与调用者绑定，CLI 同 token 同域名照样限流
/// - **HTTP 401 / 403** → ❌ 认证问题，CLI 共用 auth.json 同样失败
/// - **HTTP 4xx 其它 (400/404 等)** → ❌ 请求本身问题，不会被 CLI 修正
/// - **其它 ProviderError (NoData / ParseFailed / ConfigMissing / SessionExpired)** → ❌
fn should_fallback_to_cli(err: &anyhow::Error) -> bool {
    if let Some(http) = err.downcast_ref::<HttpError>() {
        return match http {
            HttpError::Timeout => true,
            HttpError::Transport(_) => true,
            HttpError::HttpStatus { code, .. } => (500..=599).contains(code),
        };
    }
    matches!(
        ProviderError::classify(err),
        ProviderError::Timeout | ProviderError::NetworkFailed { .. }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_auth_error_detection_only_matches_401_and_403() {
        let auth_401: anyhow::Error = HttpError::HttpStatus {
            code: 401,
            body: "Unauthorized".into(),
        }
        .into();
        let auth_403: anyhow::Error = HttpError::HttpStatus {
            code: 403,
            body: "Forbidden".into(),
        }
        .into();
        let other_status: anyhow::Error = HttpError::HttpStatus {
            code: 500,
            body: "Server Error".into(),
        }
        .into();

        assert!(is_auth_api_error(&auth_401));
        assert!(is_auth_api_error(&auth_403));
        assert!(!is_auth_api_error(&other_status));
    }

    #[test]
    fn codex_refresh_failure_returns_structured_auth_error() {
        let usage_error: anyhow::Error = HttpError::HttpStatus {
            code: 401,
            body: "Unauthorized".into(),
        }
        .into();
        let refresh_error = ProviderError::fetch_failed("refresh token rejected");

        let provider_error = auth_refresh_failed_error(&usage_error, &refresh_error);

        assert!(matches!(
            provider_error,
            ProviderError::SessionExpired {
                advice: Some(FailureAdvice::ReloginCli { ref cli })
            } if cli == "codex"
        ));
    }

    // ────────────────────────────────────────────────────────────────────────
    // should_fallback_to_cli：决策矩阵
    // ────────────────────────────────────────────────────────────────────────

    #[test]
    fn fallback_eligible_for_timeout() {
        let err: anyhow::Error = HttpError::Timeout.into();
        assert!(should_fallback_to_cli(&err));
    }

    #[test]
    fn fallback_eligible_for_network_failure() {
        let err: anyhow::Error = HttpError::Transport("dns lookup failed".into()).into();
        assert!(should_fallback_to_cli(&err));
    }

    #[test]
    fn fallback_eligible_for_5xx() {
        let err: anyhow::Error = HttpError::HttpStatus {
            code: 503,
            body: "Service Unavailable".into(),
        }
        .into();
        assert!(should_fallback_to_cli(&err));
    }

    #[test]
    fn fallback_not_eligible_for_429_rate_limited() {
        // 429 不兑底：CLI 同 token 同域名，服务端还会返回 429，白费一次 PTY spawn。
        let err: anyhow::Error = HttpError::HttpStatus {
            code: 429,
            body: "rate limited".into(),
        }
        .into();
        assert!(!should_fallback_to_cli(&err));
    }

    #[test]
    fn fallback_not_eligible_for_4xx_other_than_auth_or_429() {
        // 400 / 404 等请求问题不会被 CLI 修正。
        let err_400: anyhow::Error = HttpError::HttpStatus {
            code: 400,
            body: "Bad Request".into(),
        }
        .into();
        let err_404: anyhow::Error = HttpError::HttpStatus {
            code: 404,
            body: "Not Found".into(),
        }
        .into();
        assert!(!should_fallback_to_cli(&err_400));
        assert!(!should_fallback_to_cli(&err_404));
    }

    #[test]
    fn fallback_eligible_for_502_503_504() {
        // 全部 5xx 都兑底，不仅是 503。
        for code in [500, 502, 503, 504, 599] {
            let err: anyhow::Error = HttpError::HttpStatus {
                code,
                body: format!("server error {code}"),
            }
            .into();
            assert!(
                should_fallback_to_cli(&err),
                "expected fallback for HTTP {code}"
            );
        }
    }

    #[test]
    fn fallback_not_eligible_for_auth_errors() {
        let err_401: anyhow::Error = HttpError::HttpStatus {
            code: 401,
            body: "Unauthorized".into(),
        }
        .into();
        let err_403: anyhow::Error = HttpError::HttpStatus {
            code: 403,
            body: "Forbidden".into(),
        }
        .into();
        assert!(!should_fallback_to_cli(&err_401));
        assert!(!should_fallback_to_cli(&err_403));
    }

    #[test]
    fn fallback_not_eligible_for_session_expired() {
        // 模拟 refresh 失败后构造的 SessionExpired：CLI 也共用 auth.json，无意义重试。
        let err: anyhow::Error = ProviderError::SessionExpired {
            advice: Some(FailureAdvice::ReloginCli {
                cli: "codex".to_string(),
            }),
        }
        .into();
        assert!(!should_fallback_to_cli(&err));
    }

    #[test]
    fn fallback_not_eligible_for_no_data_or_parse_failed() {
        let no_data: anyhow::Error = ProviderError::NoData.into();
        let parse_failed: anyhow::Error = ProviderError::parse_failed("usage").into();
        let config_missing: anyhow::Error = ProviderError::config_missing("auth.json").into();
        assert!(!should_fallback_to_cli(&no_data));
        assert!(!should_fallback_to_cli(&parse_failed));
        assert!(!should_fallback_to_cli(&config_missing));
    }
}
