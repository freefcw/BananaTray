use crate::providers::{ProviderError, ProviderResult};
use serde::Deserialize;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Deserialize)]
pub(super) struct OAuthCredentials {
    pub access_token: Option<String>,
    #[serde(rename = "expiry_date")]
    pub expiry_date_ms: Option<f64>,
    pub id_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GeminiSettings {
    security: GeminiSecurity,
}

#[derive(Debug, Deserialize)]
struct GeminiSecurity {
    auth: GeminiAuth,
}

#[derive(Debug, Deserialize)]
struct GeminiAuth {
    #[serde(rename = "selectedType")]
    selected_type: String,
}

pub(super) fn credentials_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("~"))
        .join(".gemini/oauth_creds.json")
}

fn settings_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("~"))
        .join(".gemini/settings.json")
}

pub(super) fn load_credentials() -> ProviderResult<OAuthCredentials> {
    let path = credentials_path();
    let content = std::fs::read_to_string(&path)
        .map_err(|_| ProviderError::config_missing("~/.gemini/oauth_creds.json"))?;
    let creds: OAuthCredentials = serde_json::from_str(&content)
        .map_err(|_| ProviderError::parse_failed("oauth_creds.json"))?;
    Ok(creds)
}

pub(super) fn check_auth_type() -> ProviderResult<()> {
    let path = settings_path();
    if let Ok(content) = std::fs::read_to_string(&path) {
        check_auth_type_from_content(&content)
    } else {
        Ok(())
    }
}

pub(super) fn check_auth_type_from_content(content: &str) -> ProviderResult<()> {
    let settings: GeminiSettings =
        serde_json::from_str(content).map_err(|_| ProviderError::parse_failed("settings.json"))?;
    match settings.security.auth.selected_type.as_str() {
        "oauth-personal" | "unknown" => Ok(()),
        "api-key" => Err(ProviderError::config_missing(
            "Gemini API key is not supported, please use Google account (OAuth) login",
        )),
        "vertex-ai" => Err(ProviderError::config_missing(
            "Gemini Vertex AI is not supported, please use Google account (OAuth) login",
        )),
        _ => Ok(()),
    }
}

pub(super) fn refresh_token_via_cli() -> ProviderResult<()> {
    let output = Command::new("gemini").args(["--version"]).output();

    if output.is_err() {
        return Err(ProviderError::cli_not_found("gemini"));
    }

    let output = Command::new("sh")
        .args(["-c", "echo '/quit' | gemini 2>/dev/null || true"])
        .output()
        .map_err(|err| {
            ProviderError::fetch_failed(&format!("run gemini CLI for token refresh: {err}"))
        })?;

    if !output.status.success() {
        log::warn!(target: "providers", "gemini CLI token refresh exited with: {:?}", output.status);
    }

    let creds_path = credentials_path();
    let before = std::fs::metadata(&creds_path)
        .and_then(|m| m.modified())
        .ok();

    for _ in 0..10 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        let after = std::fs::metadata(&creds_path)
            .and_then(|m| m.modified())
            .ok();
        if after != before {
            return Ok(());
        }
    }

    log::warn!(target: "providers", "Gemini CLI token refresh: credential file not updated after 1s poll");
    Ok(())
}
