use crate::models::FailureAdvice;
use crate::platform::atomic_file::write_private_file_atomically;
use crate::providers::common::http_client;
use crate::providers::{ProviderError, ProviderResult};
use crate::utils::time_utils;
use serde_json::Value;
use std::path::{Path, PathBuf};

const AUTH_FILE: &str = "auth.json";
const REFRESH_SKEW_SECS: i64 = 5 * 60;
const DEFAULT_ISSUER: &str = "https://auth.x.ai";
const DEFAULT_EXPIRES_IN_SECS: i64 = 1800;

#[derive(Debug, Clone)]
pub(super) struct GrokCredentials {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub email: Option<String>,
    pub session_key: String,
    pub expires_at: Option<String>,
    pub oidc_issuer: Option<String>,
    pub oidc_client_id: Option<String>,
}

pub(super) fn auth_path() -> PathBuf {
    grok_home().join(AUTH_FILE)
}

fn grok_home() -> PathBuf {
    if let Ok(home) = std::env::var("GROK_HOME") {
        let trimmed = home.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".grok")
}

pub(super) fn load_credentials() -> ProviderResult<GrokCredentials> {
    load_credentials_from_path(&auth_path())
}

pub(super) fn load_credentials_from_path(path: &Path) -> ProviderResult<GrokCredentials> {
    let content = std::fs::read_to_string(path)
        .map_err(|_| ProviderError::config_missing("~/.grok/auth.json"))?;
    parse_auth_json(&content)
}

pub(super) fn parse_auth_json(content: &str) -> ProviderResult<GrokCredentials> {
    let root: Value =
        serde_json::from_str(content).map_err(|_| ProviderError::parse_failed("grok auth.json"))?;
    let (session_key, session) = pick_session_entry(&root)?;
    let access_token = json_text(session, "key")
        .ok_or_else(|| ProviderError::config_missing("grok access token in auth.json"))?;
    Ok(GrokCredentials {
        access_token,
        refresh_token: json_text(session, "refresh_token"),
        email: json_text(session, "email"),
        session_key: session_key.clone(),
        expires_at: json_text(session, "expires_at"),
        oidc_issuer: json_text(session, "oidc_issuer"),
        oidc_client_id: json_text(session, "oidc_client_id"),
    })
}

fn token_needs_refresh(expires_at: Option<&str>, now_epoch: i64) -> bool {
    let Some(expires_at) = expires_at else {
        return false;
    };
    match time_utils::parse_iso8601_to_epoch(expires_at) {
        Some(expiry) => expiry - now_epoch <= REFRESH_SKEW_SECS,
        None => false,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RefreshedTokens {
    access_token: String,
    refresh_token: Option<String>,
    expires_at: String,
}

fn apply_refreshed_tokens(
    content: &str,
    session_key: &str,
    tokens: &RefreshedTokens,
) -> ProviderResult<String> {
    let mut root: Value =
        serde_json::from_str(content).map_err(|_| ProviderError::parse_failed("grok auth.json"))?;
    let session = root
        .get_mut(session_key)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| ProviderError::parse_failed("updated grok auth.json"))?;
    session.insert("key".into(), Value::String(tokens.access_token.clone()));
    session.insert(
        "expires_at".into(),
        Value::String(tokens.expires_at.clone()),
    );
    if let Some(refresh_token) = tokens.refresh_token.as_deref() {
        session.insert(
            "refresh_token".into(),
            Value::String(refresh_token.to_string()),
        );
    }
    serde_json::to_string_pretty(&root)
        .map_err(|_| ProviderError::parse_failed("updated grok auth.json"))
}

fn save_refreshed_tokens(
    path: &Path,
    session_key: &str,
    tokens: &RefreshedTokens,
) -> ProviderResult<()> {
    let content = std::fs::read_to_string(path)
        .map_err(|err| ProviderError::fetch_failed(&format!("read auth.json: {err}")))?;
    let serialized = apply_refreshed_tokens(&content, session_key, tokens)?;
    write_private_file_atomically(path, serialized.as_bytes()).map_err(|err| {
        ProviderError::fetch_failed(&format!("write auth.json atomically: {err}"))
    })?;
    Ok(())
}

fn token_url(issuer: &str) -> String {
    format!("{}/oauth2/token", issuer.trim_end_matches('/'))
}

fn parse_refresh_response(body: &str, now_epoch: i64) -> ProviderResult<RefreshedTokens> {
    let resp: Value = serde_json::from_str(body)
        .map_err(|_| ProviderError::parse_failed("token refresh response"))?;
    let access_token = json_text(&resp, "access_token")
        .ok_or_else(|| ProviderError::parse_failed("missing access_token in refresh response"))?;
    let refresh_token = json_text(&resp, "refresh_token");
    let expires_in = resp
        .get("expires_in")
        .and_then(Value::as_i64)
        .filter(|secs| *secs > 0)
        .unwrap_or(DEFAULT_EXPIRES_IN_SECS);
    Ok(RefreshedTokens {
        access_token,
        refresh_token,
        expires_at: time_utils::epoch_to_iso8601((now_epoch + expires_in) as u64),
    })
}

fn form_encode(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(key, value)| format!("{key}={}", percent_encode(value)))
        .collect::<Vec<_>>()
        .join("&")
}

fn percent_encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(*byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// 用 refresh_token 换新的 access token，并原子写回 `auth.json`。
fn refresh_access_token(credentials: &GrokCredentials) -> ProviderResult<()> {
    let refresh_token = credentials
        .refresh_token
        .as_deref()
        .ok_or_else(session_expired)?;
    let client_id = credentials
        .oidc_client_id
        .as_deref()
        .ok_or_else(session_expired)?;
    let issuer = credentials.oidc_issuer.as_deref().unwrap_or(DEFAULT_ISSUER);
    let body = form_encode(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", client_id),
    ]);
    let response = http_client::post_form(&token_url(issuer), &[], &body)
        .map_err(|err| ProviderError::classify(&err))?;
    let refreshed = parse_refresh_response(&response, time_utils::now_epoch_secs())?;
    save_refreshed_tokens(&auth_path(), &credentials.session_key, &refreshed)
}

/// 刷新成功后尽量读回磁盘上的最新 session。读失败时保留内存里的旧值，
/// 让调用方仍可用当前 token 再试一次，而不是直接当成过期。
pub(super) fn refresh_and_reload(credentials: &mut GrokCredentials) -> ProviderResult<()> {
    refresh_access_token(credentials)?;
    if let Ok(reloaded) = load_credentials() {
        *credentials = reloaded;
    }
    Ok(())
}

/// 过期或即将过期时刷新；失败则保留旧凭证，让后续 401 再判定。
pub(super) fn ensure_access_token(credentials: &mut GrokCredentials) {
    if !token_needs_refresh(
        credentials.expires_at.as_deref(),
        time_utils::now_epoch_secs(),
    ) {
        return;
    }
    if let Err(err) = refresh_and_reload(credentials) {
        log::warn!(
            target: "providers",
            "grok access token refresh failed: {err:?}; keeping existing credentials"
        );
    }
}

pub(super) fn session_expired() -> ProviderError {
    ProviderError::session_expired(Some(FailureAdvice::ReloginCli {
        cli: "grok".to_string(),
    }))
}

fn pick_session_entry(root: &Value) -> ProviderResult<(&String, &Value)> {
    let object = root
        .as_object()
        .ok_or_else(|| ProviderError::parse_failed("grok auth.json"))?;
    let mut best: Option<(&String, &Value, i64)> = None;
    for (key, session) in object {
        let Some(access) = session.get("key").and_then(Value::as_str) else {
            continue;
        };
        if access.trim().is_empty() {
            continue;
        }
        let expiry = session
            .get("expires_at")
            .and_then(Value::as_str)
            .and_then(time_utils::parse_iso8601_to_epoch)
            .unwrap_or(0);
        if best.is_none_or(|(_, _, best_expiry)| expiry >= best_expiry) {
            best = Some((key, session, expiry));
        }
    }
    best.map(|(key, session, _)| (key, session))
        .ok_or_else(|| ProviderError::config_missing("grok access token in auth.json"))
}

fn json_text(value: &Value, field: &str) -> Option<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_auth(expires_at: &str) -> String {
        format!(
            r#"{{
                "https://auth.x.ai::abc": {{
                    "key": "access-token",
                    "auth_mode": "oidc",
                    "email": "user@example.com",
                    "refresh_token": "refresh-token",
                    "expires_at": "{expires_at}",
                    "oidc_issuer": "https://auth.x.ai",
                    "oidc_client_id": "client-id"
                }}
            }}"#
        )
    }

    #[test]
    fn parses_oidc_session_from_auth_json() {
        let creds = parse_auth_json(&sample_auth("2026-08-18T09:20:48.769144Z")).unwrap();
        assert_eq!(creds.access_token, "access-token");
        assert_eq!(creds.refresh_token.as_deref(), Some("refresh-token"));
        assert_eq!(creds.email.as_deref(), Some("user@example.com"));
        assert_eq!(creds.session_key, "https://auth.x.ai::abc");
        assert_eq!(creds.oidc_client_id.as_deref(), Some("client-id"));
    }

    #[test]
    fn prefers_session_with_later_expiry() {
        let content = r#"{
            "https://auth.x.ai::old": {
                "key": "old-token",
                "expires_at": "2026-08-18T08:00:00Z"
            },
            "https://auth.x.ai::new": {
                "key": "new-token",
                "expires_at": "2026-08-18T10:00:00Z"
            }
        }"#;
        let creds = parse_auth_json(content).unwrap();
        assert_eq!(creds.access_token, "new-token");
        assert_eq!(creds.session_key, "https://auth.x.ai::new");
    }

    #[test]
    fn rejects_missing_access_token() {
        let err = parse_auth_json(r#"{"https://auth.x.ai::abc":{"email":"a@b.c"}}"#).unwrap_err();
        assert_eq!(
            err,
            ProviderError::config_missing("grok access token in auth.json")
        );
    }

    #[test]
    fn token_needs_refresh_within_skew() {
        let now = time_utils::parse_iso8601_to_epoch("2026-08-18T09:00:00Z").unwrap();
        assert!(token_needs_refresh(Some("2026-08-18T09:04:00Z"), now));
        assert!(!token_needs_refresh(Some("2026-08-18T09:10:00Z"), now));
        assert!(!token_needs_refresh(None, now));
    }

    #[test]
    fn parse_refresh_response_reads_tokens_and_expiry() {
        let now = time_utils::parse_iso8601_to_epoch("2026-08-18T09:00:00Z").unwrap();
        let refreshed = parse_refresh_response(
            r#"{"access_token":"new-access","refresh_token":"new-refresh","expires_in":1800}"#,
            now,
        )
        .unwrap();
        assert_eq!(refreshed.access_token, "new-access");
        assert_eq!(refreshed.refresh_token.as_deref(), Some("new-refresh"));
        assert_eq!(refreshed.expires_at, "2026-08-18T09:30:00.000Z");
    }

    #[test]
    fn form_encode_percent_encodes_refresh_token() {
        assert_eq!(
            form_encode(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", "a+b=c"),
                ("client_id", "id")
            ]),
            "grant_type=refresh_token&refresh_token=a%2Bb%3Dc&client_id=id"
        );
    }

    #[test]
    fn token_url_appends_oauth2_token_path() {
        assert_eq!(
            token_url("https://auth.x.ai"),
            "https://auth.x.ai/oauth2/token"
        );
        assert_eq!(
            token_url("https://auth.example.com/"),
            "https://auth.example.com/oauth2/token"
        );
    }

    #[test]
    fn apply_refreshed_tokens_updates_selected_session() {
        let updated = apply_refreshed_tokens(
            &sample_auth("2026-08-18T09:20:48Z"),
            "https://auth.x.ai::abc",
            &RefreshedTokens {
                access_token: "new-access".into(),
                refresh_token: Some("new-refresh".into()),
                expires_at: "2026-08-18T10:20:48Z".into(),
            },
        )
        .unwrap();
        let creds = parse_auth_json(&updated).unwrap();
        assert_eq!(creds.access_token, "new-access");
        assert_eq!(creds.refresh_token.as_deref(), Some("new-refresh"));
        assert_eq!(creds.expires_at.as_deref(), Some("2026-08-18T10:20:48Z"));
        assert_eq!(creds.email.as_deref(), Some("user@example.com"));
    }
}
