use crate::providers::ProviderError;
use crate::utils::http_client;
use crate::utils::time_utils;
use anyhow::{Context, Result};
use std::path::PathBuf;

const TOKEN_MAX_AGE_SECS: i64 = 8 * 24 * 60 * 60;

pub(super) fn auth_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".codex")
        .join("auth.json")
}

pub(super) fn load_credentials() -> Result<(String, String, Option<String>)> {
    let path = auth_path();
    let content = std::fs::read_to_string(&path)
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

    let last_refresh = json
        .get("last_refresh")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Ok((access_token, refresh_token, last_refresh))
}

fn token_needs_refresh(last_refresh: &Option<String>) -> bool {
    let Some(ts) = last_refresh else {
        return true;
    };

    time_utils::is_older_than(ts, TOKEN_MAX_AGE_SECS)
}

pub(super) fn refresh_access_token(refresh_token: &str) -> Result<String> {
    let body = format!(
        "grant_type=refresh_token&refresh_token={}&client_id=app_EMoamEEZ73f0CkXaXp7hrann",
        refresh_token
    );

    let response_str = http_client::post_form("https://auth.openai.com/oauth/token", &[], &body)?;

    let resp: serde_json::Value =
        serde_json::from_str(&response_str).context("Failed to parse token refresh response")?;

    let new_access = resp
        .get("access_token")
        .and_then(|v| v.as_str())
        .context("No access_token in refresh response")?
        .to_string();

    let new_refresh = resp
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    save_refreshed_tokens(&new_access, new_refresh.as_deref(), refresh_token)?;

    Ok(new_access)
}

fn save_refreshed_tokens(
    access_token: &str,
    new_refresh_token: Option<&str>,
    old_refresh_token: &str,
) -> Result<()> {
    let path = auth_path();
    let content = std::fs::read_to_string(&path).unwrap_or_default();
    let mut json: serde_json::Value =
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}));

    if let Some(tokens) = json.get_mut("tokens") {
        tokens["access_token"] = serde_json::json!(access_token);
        if let Some(rt) = new_refresh_token {
            tokens["refresh_token"] = serde_json::json!(rt);
        }
    } else {
        json["tokens"] = serde_json::json!({
            "access_token": access_token,
            "refresh_token": new_refresh_token.unwrap_or(old_refresh_token),
        });
    }

    let now_str = time_utils::epoch_to_iso8601(time_utils::now_epoch_secs() as u64);
    json["last_refresh"] = serde_json::json!(now_str);

    let serialized = serde_json::to_string_pretty(&json)?;
    std::fs::write(&path, serialized).context("Failed to write updated auth.json")?;

    Ok(())
}

pub(super) fn get_valid_token() -> Result<String> {
    let (access_token, refresh_token, last_refresh) = load_credentials()?;

    if token_needs_refresh(&last_refresh) {
        match refresh_access_token(&refresh_token) {
            Ok(new_token) => Ok(new_token),
            Err(_) => Ok(access_token),
        }
    } else {
        Ok(access_token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_needs_refresh_without_timestamp() {
        assert!(token_needs_refresh(&None));
    }

    #[test]
    fn test_token_needs_refresh_with_recent_timestamp() {
        let now = time_utils::epoch_to_iso8601(time_utils::now_epoch_secs() as u64);
        assert!(!token_needs_refresh(&Some(now)));
    }
}
