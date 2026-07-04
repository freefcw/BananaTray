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
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".gemini/oauth_creds.json")
}

/// 用户可见的凭证路径描述（用于错误提示）。
pub(super) fn credentials_path_display() -> &'static str {
    "~/.gemini/oauth_creds.json"
}

fn settings_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".gemini/settings.json")
}

pub(super) fn load_credentials() -> ProviderResult<OAuthCredentials> {
    let path = credentials_path();
    let content = std::fs::read_to_string(&path)
        .map_err(|_| ProviderError::config_missing(credentials_path_display()))?;
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

    let creds_path = credentials_path();
    // before mtime 必须在 CLI 执行前读取：CLI 同步写完凭证后，
    // 再读 before 会和 after 相同，把成功刷新误报为失败。
    let before_mtime = std::fs::metadata(&creds_path)
        .and_then(|m| m.modified())
        .ok();
    let before_creds = std::fs::read_to_string(&creds_path).ok();

    let output = Command::new("sh")
        .args(["-c", "echo '/quit' | gemini 2>/dev/null || true"])
        .output()
        .map_err(|err| {
            ProviderError::fetch_failed(&format!("run gemini CLI for token refresh: {err}"))
        })?;

    if !output.status.success() {
        log::warn!(target: "providers", "gemini CLI token refresh exited with: {:?}", output.status);
    }

    // 这里假设 Gemini CLI 与 BananaTray 读取同一个 expiry_date，并且在 BananaTray
    // 判定过期时会重写凭证文件；若 CLI 将来采用不同的提前刷新窗口，这里可能误报刷新失败。
    // 双重验证：mtime 变化 OR token/expiry 实际变化。
    // mtime 在某些文件系统（如某些 NFS / 容器挂载）可能不更新，
    // 所以同时 reload 凭证内容做对比。
    if poll_credential_updated(&creds_path, before_mtime, before_creds.as_deref()) {
        Ok(())
    } else {
        log::warn!(target: "providers", "Gemini CLI token refresh: credential file not updated after 1s poll");
        Err(ProviderError::fetch_failed(
            "gemini CLI token refresh: credential file not updated after 1s poll",
        ))
    }
}

/// 检测凭证文件是否已变化（mtime 或内容）。
/// 纯判定函数，不轮询、不 sleep，便于单测。
fn credential_changed(
    path: &std::path::Path,
    before_mtime: Option<std::time::SystemTime>,
    before_content: Option<&str>,
) -> bool {
    let after_mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    if after_mtime != before_mtime {
        return true;
    }
    // mtime 未变，检查内容是否变化（兜底文件系统不更新 mtime 的情况）
    if let Some(before) = before_content {
        if let Ok(after) = std::fs::read_to_string(path) {
            return after != before;
        }
    }
    false
}

/// 轮询凭证文件，最多 10 次（每次间隔 100ms），检测 mtime 或内容是否变化。
fn poll_credential_updated(
    path: &std::path::Path,
    before_mtime: Option<std::time::SystemTime>,
    before_content: Option<&str>,
) -> bool {
    for _ in 0..10 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if credential_changed(path, before_mtime, before_content) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_credentials_path_ends_with_gemini_suffix() {
        let path = credentials_path();
        assert!(
            path.ends_with(".gemini/oauth_creds.json"),
            "credentials_path should end with .gemini/oauth_creds.json, got: {}",
            path.display()
        );
    }

    #[test]
    fn test_settings_path_ends_with_gemini_suffix() {
        let path = settings_path();
        assert!(
            path.ends_with(".gemini/settings.json"),
            "settings_path should end with .gemini/settings.json, got: {}",
            path.display()
        );
    }

    #[test]
    fn test_credentials_path_display_non_empty() {
        assert!(
            !credentials_path_display().is_empty(),
            "credentials_path_display should not be empty"
        );
    }

    #[test]
    fn test_credentials_path_display_contains_gemini() {
        assert!(
            credentials_path_display().contains(".gemini/oauth_creds.json"),
            "credentials_path_display should contain .gemini suffix, got: {}",
            credentials_path_display()
        );
    }

    #[test]
    fn test_check_auth_type_from_content_oauth_personal() {
        assert!(check_auth_type_from_content(
            r#"{"security":{"auth":{"selectedType":"oauth-personal"}}}"#
        )
        .is_ok());
    }

    #[test]
    fn test_check_auth_type_from_content_api_key_rejected() {
        let result =
            check_auth_type_from_content(r#"{"security":{"auth":{"selectedType":"api-key"}}}"#);
        assert!(result.is_err());
    }

    #[test]
    fn test_check_auth_type_from_content_invalid_json() {
        let result = check_auth_type_from_content("not json");
        assert!(result.is_err());
    }

    #[test]
    fn test_credential_changed_detects_content_change() {
        // 内容变化兜底：mtime 可能未变（某些文件系统），但内容变了应检测到。
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("oauth_creds.json");
        std::fs::write(&path, r#"{"access_token":"old"}"#).unwrap();
        let before_mtime = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .expect("read before mtime");
        let before_content = std::fs::read_to_string(&path).expect("read before content");
        std::fs::write(&path, r#"{"access_token":"new"}"#).unwrap();
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(before_mtime)
            .unwrap();
        assert_eq!(
            std::fs::metadata(&path).and_then(|m| m.modified()).ok(),
            Some(before_mtime),
            "test must force credential_changed past the mtime branch"
        );

        assert!(credential_changed(
            &path,
            Some(before_mtime),
            Some(&before_content)
        ));
    }

    #[test]
    fn test_credential_changed_returns_false_when_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("oauth_creds.json");
        std::fs::write(&path, "stable").unwrap();
        let before_mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        let before_content = std::fs::read_to_string(&path).ok();
        // 不修改文件，判定应返回 false（即时，不轮询）
        assert!(!credential_changed(
            &path,
            before_mtime,
            before_content.as_deref()
        ));
    }

    #[test]
    fn test_poll_credential_updated_detects_change() {
        // 轮询循环：改内容后第一次 sleep 即应检测到（~100ms）
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("oauth_creds.json");
        std::fs::write(&path, "old").unwrap();
        let before_mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        let before_content = std::fs::read_to_string(&path).ok();
        std::fs::write(&path, "new").unwrap();
        assert!(poll_credential_updated(
            &path,
            before_mtime,
            before_content.as_deref()
        ));
    }
}
