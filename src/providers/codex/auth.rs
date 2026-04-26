use crate::providers::common::http_client;
use crate::providers::{ProviderError, ProviderResult};
use crate::utils::time_utils;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use std::path::{Path, PathBuf};

const TOKEN_MAX_AGE_SECS: i64 = 8 * 24 * 60 * 60;

/// Codex `~/.codex/auth.json` 解析结果。
///
/// `id_token` 和 `account_id` 用于补充账户身份：JWT 可提供 email / plan；
/// `account_id` 则用于 `ChatGPT-Account-Id` 请求头注入。
#[derive(Debug, Clone)]
pub(super) struct CodexCredentials {
    pub access_token: String,
    pub refresh_token: String,
    pub id_token: Option<String>,
    pub account_id: Option<String>,
    pub last_refresh: Option<String>,
}

/// 从 credentials 提取的展示用身份字段。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct CodexAccount {
    pub email: Option<String>,
    pub plan: Option<String>,
    pub account_id: Option<String>,
}

pub(super) fn auth_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".codex")
        .join("auth.json")
}

pub(super) fn load_credentials() -> ProviderResult<CodexCredentials> {
    load_credentials_from_path(&auth_path())
}

/// 从指定路径加载 credentials（抽出以便单测注入 tempfile）。
fn load_credentials_from_path(path: &Path) -> ProviderResult<CodexCredentials> {
    let content = std::fs::read_to_string(path)
        .map_err(|_| ProviderError::config_missing("~/.codex/auth.json"))?;
    let json: serde_json::Value =
        serde_json::from_str(&content).map_err(|_| ProviderError::parse_failed("auth.json"))?;

    let tokens = json
        .get("tokens")
        .ok_or_else(|| ProviderError::config_missing("tokens in auth.json"))?;

    let access_token = tokens
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ProviderError::config_missing("access_token"))?
        .to_string();

    let refresh_token = tokens
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ProviderError::config_missing("refresh_token"))?
        .to_string();

    let id_token = tokens
        .get("id_token")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // account_id 在新版 codex auth.json 中位于顶层；兼容旧版可能放在 tokens 里。
    let account_id = json
        .get("account_id")
        .and_then(|v| v.as_str())
        .or_else(|| tokens.get("account_id").and_then(|v| v.as_str()))
        .map(|s| s.to_string());

    let last_refresh = json
        .get("last_refresh")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Ok(CodexCredentials {
        access_token,
        refresh_token,
        id_token,
        account_id,
        last_refresh,
    })
}

/// 解码 JWT payload（只读，不验签）。与 CodexBar `UsageFetcher.parseJWT` 等价。
pub(super) fn parse_jwt_payload(token: &str) -> Option<serde_json::Value> {
    let mut parts = token.split('.');
    let _header = parts.next()?;
    let payload_b64 = parts.next()?;
    let bytes = URL_SAFE_NO_PAD.decode(payload_b64).ok()?;
    serde_json::from_slice::<serde_json::Value>(&bytes).ok()
}

/// 从 credentials 提取展示用账户字段。
///
/// 优先级与 CodexBar `CodexReconciledState.oauthIdentity` 对齐：
/// - email: JWT payload 的 `email` 或 `https://api.openai.com/profile.email`
/// - plan: JWT payload 的 `https://api.openai.com/auth.chatgpt_plan_type` 或顶层 `chatgpt_plan_type`
/// - account_id: credentials 自带优先，否则从 JWT 的 `https://api.openai.com/auth.chatgpt_account_id` 回退
pub(super) fn resolve_account(credentials: &CodexCredentials) -> CodexAccount {
    let payload = credentials.id_token.as_deref().and_then(parse_jwt_payload);
    let payload_ref = payload.as_ref();

    let profile = payload_ref.and_then(|p| p.get("https://api.openai.com/profile"));
    let auth = payload_ref.and_then(|p| p.get("https://api.openai.com/auth"));

    let email = normalize_field(
        payload_ref
            .and_then(|p| p.get("email").and_then(|v| v.as_str()))
            .or_else(|| profile.and_then(|p| p.get("email").and_then(|v| v.as_str()))),
    );

    let plan = normalize_field(
        auth.and_then(|a| a.get("chatgpt_plan_type").and_then(|v| v.as_str()))
            .or_else(|| {
                payload_ref.and_then(|p| p.get("chatgpt_plan_type").and_then(|v| v.as_str()))
            }),
    );

    let account_id = normalize_field(credentials.account_id.as_deref()).or_else(|| {
        normalize_field(auth.and_then(|a| a.get("chatgpt_account_id").and_then(|v| v.as_str())))
    });

    CodexAccount {
        email,
        plan,
        account_id,
    }
}

/// 去空白和空串，与 CodexBar `normalizedCodexAccountField` 等价。
pub(super) fn normalize_field(value: Option<&str>) -> Option<String> {
    let trimmed = value?.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn token_needs_refresh(last_refresh: &Option<String>) -> bool {
    let Some(ts) = last_refresh else {
        return true;
    };

    time_utils::is_older_than(ts, TOKEN_MAX_AGE_SECS)
}

pub(super) fn refresh_access_token(refresh_token: &str) -> ProviderResult<String> {
    let body = format!(
        "grant_type=refresh_token&refresh_token={}&client_id=app_EMoamEEZ73f0CkXaXp7hrann",
        refresh_token
    );

    let response_str = http_client::post_form("https://auth.openai.com/oauth/token", &[], &body)
        .map_err(|err| ProviderError::classify(&err))?;

    let resp: serde_json::Value = serde_json::from_str(&response_str)
        .map_err(|_| ProviderError::parse_failed("token refresh response"))?;

    let new_access = resp
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ProviderError::parse_failed("missing access_token in refresh response"))?
        .to_string();

    let new_refresh = resp
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // 接收新 id_token 并写回，对齐 CodexBar `CodexOAuthCredentialsStore.save` 的行为，
    // 避免 email/plan 字段随旧 id_token 逐渐陈旧。
    let new_id_token = resp
        .get("id_token")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    save_refreshed_tokens(
        &auth_path(),
        &new_access,
        new_refresh.as_deref(),
        new_id_token.as_deref(),
        refresh_token,
    )?;

    Ok(new_access)
}

/// 将刷新后的 token 写回本地 auth.json。抽出 path 参数以便单测。
fn save_refreshed_tokens(
    path: &Path,
    access_token: &str,
    new_refresh_token: Option<&str>,
    new_id_token: Option<&str>,
    old_refresh_token: &str,
) -> ProviderResult<()> {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    let mut json: serde_json::Value =
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}));

    if let Some(tokens) = json.get_mut("tokens") {
        tokens["access_token"] = serde_json::json!(access_token);
        if let Some(rt) = new_refresh_token {
            tokens["refresh_token"] = serde_json::json!(rt);
        }
        if let Some(id) = new_id_token {
            tokens["id_token"] = serde_json::json!(id);
        }
    } else {
        let mut tokens_obj = serde_json::json!({
            "access_token": access_token,
            "refresh_token": new_refresh_token.unwrap_or(old_refresh_token),
        });
        if let Some(id) = new_id_token {
            tokens_obj["id_token"] = serde_json::json!(id);
        }
        json["tokens"] = tokens_obj;
    }

    let now_str = time_utils::epoch_to_iso8601(time_utils::now_epoch_secs() as u64);
    json["last_refresh"] = serde_json::json!(now_str);

    let serialized = serde_json::to_string_pretty(&json)
        .map_err(|_| ProviderError::parse_failed("updated auth.json"))?;
    std::fs::write(path, serialized)
        .map_err(|err| ProviderError::fetch_failed(&format!("write auth.json: {err}")))?;

    Ok(())
}

/// 若 token 过期则主动刷新并 reload credentials，确保 `access_token`、`refresh_token`、
/// `id_token` 都反映最新的轮转结果。
///
/// 这一点很关键：OAuth 服务端可能在刷新时轮转 `refresh_token`，
/// 若不 reload，后续被动刷新会使用已失效的旧 token，把可恢复的状态误报成 `SessionExpired`；
/// 同时 `resolve_account` 也会继续用陈旧的 `id_token`，导致 `ChatGPT-Account-Id`
/// 和展示用的 email / plan 滞后一轮。
///
/// 刷新或 reload 任一失败都保持 `credentials` 不变：
/// - 刷新失败：旧 token 仍可能被服务端接受；若真已失效，后续 401 会由 `fetch_usage` 重试
/// - reload 失败：文件刚被 `save_refreshed_tokens` 写过，再次读取失败属极罕见
pub(super) fn ensure_access_token(credentials: &mut CodexCredentials) {
    if !token_needs_refresh(&credentials.last_refresh) {
        return;
    }
    if refresh_access_token(&credentials.refresh_token).is_err() {
        return;
    }
    if let Ok(reloaded) = load_credentials() {
        *credentials = reloaded;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;

    #[test]
    fn test_token_needs_refresh_without_timestamp() {
        assert!(token_needs_refresh(&None));
    }

    #[test]
    fn test_token_needs_refresh_with_recent_timestamp() {
        let now = time_utils::epoch_to_iso8601(time_utils::now_epoch_secs() as u64);
        assert!(!token_needs_refresh(&Some(now)));
    }

    /// 构造不经签名的 JWT（header.payload.signature），payload 为给定 JSON。
    fn make_jwt(payload: serde_json::Value) -> String {
        let header = URL_SAFE_NO_PAD.encode(b"{\"alg\":\"none\"}");
        let body = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).unwrap());
        let sig = URL_SAFE_NO_PAD.encode(b"sig");
        format!("{header}.{body}.{sig}")
    }

    #[test]
    fn test_parse_jwt_payload_extracts_claims() {
        let token = make_jwt(serde_json::json!({ "email": "user@example.com" }));
        let payload = parse_jwt_payload(&token).expect("payload");
        assert_eq!(
            payload.get("email").and_then(|v| v.as_str()),
            Some("user@example.com")
        );
    }

    #[test]
    fn test_parse_jwt_payload_malformed_returns_none() {
        assert!(parse_jwt_payload("not-a-jwt").is_none());
        assert!(parse_jwt_payload("only.two").is_none());
        assert!(parse_jwt_payload("a.!!!notbase64!!!.c").is_none());
    }

    #[test]
    fn test_resolve_account_prefers_jwt_email_and_plan() {
        let token = make_jwt(serde_json::json!({
            "email": " user@example.com ",
            "https://api.openai.com/auth": {
                "chatgpt_plan_type": "pro",
                "chatgpt_account_id": "acct_jwt"
            }
        }));
        let creds = CodexCredentials {
            access_token: "a".into(),
            refresh_token: "r".into(),
            id_token: Some(token),
            account_id: None,
            last_refresh: None,
        };
        let account = resolve_account(&creds);
        assert_eq!(account.email.as_deref(), Some("user@example.com"));
        assert_eq!(account.plan.as_deref(), Some("pro"));
        assert_eq!(account.account_id.as_deref(), Some("acct_jwt"));
    }

    #[test]
    fn test_resolve_account_profile_email_fallback() {
        let token = make_jwt(serde_json::json!({
            "https://api.openai.com/profile": { "email": "profile@example.com" }
        }));
        let creds = CodexCredentials {
            access_token: "a".into(),
            refresh_token: "r".into(),
            id_token: Some(token),
            account_id: None,
            last_refresh: None,
        };
        let account = resolve_account(&creds);
        assert_eq!(account.email.as_deref(), Some("profile@example.com"));
        assert!(account.plan.is_none());
    }

    #[test]
    fn test_resolve_account_prefers_top_level_account_id() {
        // 根据 CodexBar 的行为：credentials.accountId 比 JWT 中的 chatgpt_account_id 优先。
        let token = make_jwt(serde_json::json!({
            "https://api.openai.com/auth": { "chatgpt_account_id": "from_jwt" }
        }));
        let creds = CodexCredentials {
            access_token: "a".into(),
            refresh_token: "r".into(),
            id_token: Some(token),
            account_id: Some("from_auth_json".into()),
            last_refresh: None,
        };
        let account = resolve_account(&creds);
        assert_eq!(account.account_id.as_deref(), Some("from_auth_json"));
    }

    #[test]
    fn test_resolve_account_without_id_token_returns_empty() {
        let creds = CodexCredentials {
            access_token: "a".into(),
            refresh_token: "r".into(),
            id_token: None,
            account_id: None,
            last_refresh: None,
        };
        assert_eq!(resolve_account(&creds), CodexAccount::default());
    }

    #[test]
    fn test_resolve_account_account_id_only_from_auth_json() {
        let creds = CodexCredentials {
            access_token: "a".into(),
            refresh_token: "r".into(),
            id_token: None,
            account_id: Some("  acct_abc ".into()),
            last_refresh: None,
        };
        let account = resolve_account(&creds);
        assert_eq!(account.account_id.as_deref(), Some("acct_abc"));
    }

    #[test]
    fn test_normalize_field_trims_and_rejects_empty() {
        assert_eq!(normalize_field(Some(" x ")).as_deref(), Some("x"));
        assert!(normalize_field(Some("   ")).is_none());
        assert!(normalize_field(Some("")).is_none());
        assert!(normalize_field(None).is_none());
    }

    #[test]
    fn test_resolve_account_plan_fallback_to_top_level() {
        // JWT 没有 `auth.chatgpt_plan_type`，但顶层有 `chatgpt_plan_type`。
        let token = make_jwt(serde_json::json!({
            "email": "u@example.com",
            "chatgpt_plan_type": "plus"
        }));
        let creds = CodexCredentials {
            access_token: "a".into(),
            refresh_token: "r".into(),
            id_token: Some(token),
            account_id: None,
            last_refresh: None,
        };
        let account = resolve_account(&creds);
        assert_eq!(account.plan.as_deref(), Some("plus"));
        assert_eq!(account.email.as_deref(), Some("u@example.com"));
    }

    #[test]
    fn test_resolve_account_no_email_anywhere() {
        let token = make_jwt(serde_json::json!({
            "https://api.openai.com/auth": { "chatgpt_plan_type": "pro" }
        }));
        let creds = CodexCredentials {
            access_token: "a".into(),
            refresh_token: "r".into(),
            id_token: Some(token),
            account_id: None,
            last_refresh: None,
        };
        let account = resolve_account(&creds);
        assert!(account.email.is_none());
        assert_eq!(account.plan.as_deref(), Some("pro"));
    }

    // ========================================================================
    // load_credentials_from_path：文件 I/O 路径测试
    // ========================================================================

    fn write_auth_json(value: serde_json::Value) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("auth.json");
        std::fs::write(&path, serde_json::to_string_pretty(&value).unwrap())
            .expect("write auth.json");
        (dir, path)
    }

    #[test]
    fn test_load_credentials_reads_all_fields() {
        let (_dir, path) = write_auth_json(serde_json::json!({
            "tokens": {
                "access_token": "at",
                "refresh_token": "rt",
                "id_token": "id"
            },
            "account_id": "acct_top",
            "last_refresh": "2026-01-01T00:00:00Z"
        }));
        let creds = load_credentials_from_path(&path).expect("load");
        assert_eq!(creds.access_token, "at");
        assert_eq!(creds.refresh_token, "rt");
        assert_eq!(creds.id_token.as_deref(), Some("id"));
        assert_eq!(creds.account_id.as_deref(), Some("acct_top"));
        assert_eq!(creds.last_refresh.as_deref(), Some("2026-01-01T00:00:00Z"));
    }

    #[test]
    fn test_load_credentials_legacy_account_id_in_tokens() {
        // 旧版兼容：account_id 位于 tokens 下。
        let (_dir, path) = write_auth_json(serde_json::json!({
            "tokens": {
                "access_token": "at",
                "refresh_token": "rt",
                "account_id": "acct_legacy"
            }
        }));
        let creds = load_credentials_from_path(&path).expect("load");
        assert_eq!(creds.account_id.as_deref(), Some("acct_legacy"));
    }

    #[test]
    fn test_load_credentials_top_level_beats_tokens_nested() {
        // 顶层与 tokens 都有 account_id 时，顶层优先。
        let (_dir, path) = write_auth_json(serde_json::json!({
            "tokens": {
                "access_token": "at",
                "refresh_token": "rt",
                "account_id": "acct_legacy"
            },
            "account_id": "acct_top"
        }));
        let creds = load_credentials_from_path(&path).expect("load");
        assert_eq!(creds.account_id.as_deref(), Some("acct_top"));
    }

    #[test]
    fn test_load_credentials_missing_file_returns_config_missing() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("nope.json");
        let err = load_credentials_from_path(&missing).unwrap_err();
        assert!(matches!(err, ProviderError::ConfigMissing { .. }));
    }

    #[test]
    fn test_load_credentials_invalid_json_returns_parse_failed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth.json");
        std::fs::write(&path, "not json").unwrap();
        let err = load_credentials_from_path(&path).unwrap_err();
        assert!(matches!(err, ProviderError::ParseFailed { .. }));
    }

    #[test]
    fn test_load_credentials_missing_tokens_object() {
        let (_dir, path) = write_auth_json(serde_json::json!({ "account_id": "x" }));
        let err = load_credentials_from_path(&path).unwrap_err();
        assert!(matches!(err, ProviderError::ConfigMissing { .. }));
    }

    #[test]
    fn test_load_credentials_missing_access_token() {
        let (_dir, path) = write_auth_json(serde_json::json!({
            "tokens": { "refresh_token": "rt" }
        }));
        let err = load_credentials_from_path(&path).unwrap_err();
        assert!(matches!(err, ProviderError::ConfigMissing { .. }));
    }

    #[test]
    fn test_load_credentials_missing_refresh_token() {
        let (_dir, path) = write_auth_json(serde_json::json!({
            "tokens": { "access_token": "at" }
        }));
        let err = load_credentials_from_path(&path).unwrap_err();
        assert!(matches!(err, ProviderError::ConfigMissing { .. }));
    }

    // ========================================================================
    // save_refreshed_tokens：持久化 id_token 行为
    // ========================================================================

    #[test]
    fn test_save_refreshed_tokens_writes_id_token() {
        let (_dir, path) = write_auth_json(serde_json::json!({
            "tokens": {
                "access_token": "old_at",
                "refresh_token": "old_rt",
                "id_token": "old_id"
            }
        }));

        save_refreshed_tokens(&path, "new_at", Some("new_rt"), Some("new_id"), "old_rt")
            .expect("save");

        let reloaded = load_credentials_from_path(&path).expect("reload");
        assert_eq!(reloaded.access_token, "new_at");
        assert_eq!(reloaded.refresh_token, "new_rt");
        assert_eq!(reloaded.id_token.as_deref(), Some("new_id"));
        assert!(reloaded.last_refresh.is_some());
    }

    #[test]
    fn test_save_refreshed_tokens_preserves_existing_id_token_when_not_provided() {
        // 刷新响应未返回 id_token 时，保留旧值不动。
        let (_dir, path) = write_auth_json(serde_json::json!({
            "tokens": {
                "access_token": "old_at",
                "refresh_token": "old_rt",
                "id_token": "old_id"
            }
        }));

        save_refreshed_tokens(&path, "new_at", None, None, "old_rt").expect("save");

        let reloaded = load_credentials_from_path(&path).expect("reload");
        assert_eq!(reloaded.access_token, "new_at");
        assert_eq!(reloaded.refresh_token, "old_rt");
        assert_eq!(reloaded.id_token.as_deref(), Some("old_id"));
    }

    #[test]
    fn test_save_refreshed_tokens_creates_tokens_if_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth.json");
        std::fs::write(&path, "{}").unwrap();

        save_refreshed_tokens(&path, "at", Some("rt"), Some("id"), "old_rt").expect("save");

        let reloaded = load_credentials_from_path(&path).expect("reload");
        assert_eq!(reloaded.access_token, "at");
        assert_eq!(reloaded.refresh_token, "rt");
        assert_eq!(reloaded.id_token.as_deref(), Some("id"));
    }
}
