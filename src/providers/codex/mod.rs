mod auth;
mod client;
mod parser;

use super::{AiProvider, ProviderError};
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
use parser::parse_usage_response;

super::define_unit_provider!(CodexProvider);

fn is_auth_api_error(err: &anyhow::Error) -> bool {
    err.downcast_ref::<HttpError>()
        .is_some_and(|http_error| http_error.is_auth_error())
}

fn auth_refresh_failed_error(
    usage_error: &anyhow::Error,
    refresh_error: &anyhow::Error,
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

    async fn check_availability(&self) -> Result<()> {
        if auth_path().exists() {
            Ok(())
        } else {
            Err(ProviderError::config_missing("~/.codex/auth.json").into())
        }
    }

    async fn refresh(&self) -> Result<RefreshData> {
        let mut credentials = load_credentials()?;
        // Proactive refresh：成功会原地 reload credentials，使后续 resolve_account
        // 与被动 refresh 路径都拿到新 id_token / refresh_token。
        ensure_access_token(&mut credentials);

        let raw = fetch_usage(&mut credentials)?;

        // account 必须在最后一次 credentials 更新之后计算，
        // 这样多账号场景下 email / plan / account_id 跟请求使用的是同一份身份。
        let account = resolve_account(&credentials);
        let parsed = parse_usage_response(&raw)?;
        // 响应中的 plan_type 优先于 JWT fallback，与 CodexBar `resolvePlan` 一致。
        let account_tier = parsed.plan_type.or(account.plan);
        Ok(RefreshData::with_account(
            parsed.quotas,
            account.email,
            account_tier,
        ))
    }
}

/// 调用 usage API 一次；遇到 401/403 时被动刷新 token、reload credentials 并重试一次。
///
/// 重试前 reload 是关键：若 OAuth 服务端轮转了 refresh_token，
/// 而我们仍用内存里的旧值重试，可恢复的状态会被误报成 SessionExpired。
/// 同时 reload 也让 `ChatGPT-Account-Id` 与新 id_token 中的 `chatgpt_account_id` 同步。
fn fetch_usage(credentials: &mut CodexCredentials) -> Result<String> {
    match attempt_usage(credentials) {
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
            attempt_usage(credentials)
        }
        Err(e) => Err(e),
    }
}

/// 用当前 credentials 发起一次 usage API 调用。
fn attempt_usage(credentials: &CodexCredentials) -> Result<String> {
    let account = resolve_account(credentials);
    call_usage_api(&credentials.access_token, account.account_id.as_deref())
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
        let refresh_error = anyhow::anyhow!("refresh token rejected");

        let provider_error = auth_refresh_failed_error(&usage_error, &refresh_error);

        assert!(matches!(
            provider_error,
            ProviderError::SessionExpired {
                advice: Some(FailureAdvice::ReloginCli { ref cli })
            } if cli == "codex"
        ));
    }
}
