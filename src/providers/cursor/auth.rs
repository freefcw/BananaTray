use crate::models::FailureAdvice;
use crate::providers::common::jwt;
use crate::providers::ProviderError;
use anyhow::Result;
use std::path::PathBuf;
use std::process::Command;

pub(super) fn db_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Library/Application Support/Cursor/User/globalStorage/state.vscdb")
}

pub(super) fn read_access_token() -> Result<String> {
    let db_path = db_path();
    let db_str = db_path.to_string_lossy();

    let output = Command::new("sqlite3")
        .args([
            &*db_str,
            "SELECT value FROM ItemTable WHERE key = 'cursorAuth/accessToken'",
        ])
        .output()
        .map_err(|_| ProviderError::cli_not_found("sqlite3"))?;

    if !output.status.success() {
        return Err(
            ProviderError::fetch_failed_with_advice(FailureAdvice::CliExitFailed {
                code: output.status.code().unwrap_or(-1),
            })
            .into(),
        );
    }

    let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if token.is_empty() {
        return Err(ProviderError::auth_required(Some(FailureAdvice::LoginApp {
            app: "Cursor".to_string(),
        }))
        .into());
    }

    Ok(token)
}

pub(super) fn extract_user_id_from_jwt(token: &str) -> Result<String> {
    let payload: serde_json::Value =
        jwt::decode_payload(token).map_err(|e| ProviderError::parse_failed(&e.to_string()))?;
    let sub = payload
        .get("sub")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ProviderError::parse_failed("JWT missing 'sub' field"))?;

    Ok(sub.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_user_id_from_jwt_invalid_format() {
        assert!(extract_user_id_from_jwt("badtoken").is_err());
    }

    #[test]
    fn test_extract_user_id_from_jwt_valid() {
        use base64::Engine;
        let payload =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"sub":"user_123"}"#);
        let jwt = format!("header.{}.sig", payload);
        assert_eq!(extract_user_id_from_jwt(&jwt).unwrap(), "user_123");
    }
}
