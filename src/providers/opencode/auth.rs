use crate::providers::{ProviderError, ProviderResult};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// OpenCode Go / Zen 在 `auth.json` 中的 provider id。
/// 优先 Go；Zen key 若挂在同一 workspace 且有 Go 订阅，也可访问 `/zen/go/v1/usage`。
const AUTH_PROVIDER_IDS: &[&str] = &["opencode-go", "opencode"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct OpenCodeAuth {
    pub(super) api_key: String,
    pub(super) provider_id: String,
}

#[derive(Deserialize)]
struct AuthEntry {
    #[serde(rename = "type")]
    auth_type: Option<String>,
    key: Option<String>,
}

pub(super) fn load_auth() -> ProviderResult<OpenCodeAuth> {
    let path = find_auth_path().ok_or_else(|| {
        ProviderError::config_missing("~/.local/share/opencode/auth.json (opencode-go / opencode)")
    })?;
    load_auth_from_path(&path)
}

pub(super) fn load_auth_from_path(path: &Path) -> ProviderResult<OpenCodeAuth> {
    let content = std::fs::read_to_string(path).map_err(|_| {
        ProviderError::config_missing("~/.local/share/opencode/auth.json (opencode-go / opencode)")
    })?;
    parse_auth_json(&content)
}

fn parse_auth_json(content: &str) -> ProviderResult<OpenCodeAuth> {
    let entries: HashMap<String, AuthEntry> = serde_json::from_str(content)
        .map_err(|_| ProviderError::parse_failed("opencode auth.json"))?;

    for provider_id in AUTH_PROVIDER_IDS {
        let Some(entry) = entries.get(*provider_id) else {
            continue;
        };
        if entry.auth_type.as_deref() != Some("api") {
            continue;
        }
        if let Some(key) = entry
            .key
            .as_deref()
            .map(str::trim)
            .filter(|key| !key.is_empty())
        {
            return Ok(OpenCodeAuth {
                api_key: key.to_string(),
                provider_id: (*provider_id).to_string(),
            });
        }
    }

    Err(ProviderError::config_missing(
        "opencode-go / opencode API key in auth.json",
    ))
}

fn find_auth_path() -> Option<PathBuf> {
    auth_path_candidates()
        .into_iter()
        .find(|path| path.is_file())
}

/// OpenCode 使用 xdg-basedir：优先 `XDG_DATA_HOME`，否则 `~/.local/share`。
fn auth_path_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        let trimmed = xdg.trim();
        if !trimmed.is_empty() {
            push_unique(
                &mut candidates,
                PathBuf::from(trimmed).join("opencode").join("auth.json"),
            );
        }
    }

    if let Some(home) = dirs::home_dir() {
        push_unique(
            &mut candidates,
            home.join(".local/share/opencode/auth.json"),
        );
    }

    if let Some(data) = dirs::data_dir() {
        push_unique(&mut candidates, data.join("opencode").join("auth.json"));
    }

    candidates
}

fn push_unique(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.contains(&path) {
        paths.push(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_opencode_go_over_opencode() {
        let auth = parse_auth_json(
            r#"{
                "opencode": {"type":"api","key":"sk-zen"},
                "opencode-go": {"type":"api","key":"sk-go"}
            }"#,
        )
        .unwrap();
        assert_eq!(auth.provider_id, "opencode-go");
        assert_eq!(auth.api_key, "sk-go");
    }

    #[test]
    fn falls_back_to_opencode_zen_key() {
        let auth = parse_auth_json(r#"{"opencode":{"type":"api","key":"sk-zen"}}"#).unwrap();
        assert_eq!(auth.provider_id, "opencode");
        assert_eq!(auth.api_key, "sk-zen");
    }

    #[test]
    fn rejects_oauth_or_missing_key() {
        assert!(parse_auth_json(r#"{"opencode":{"type":"oauth","access":"x"}}"#).is_err());
        assert!(parse_auth_json(r#"{"openai":{"type":"api","key":"sk"}}"#).is_err());
        assert!(parse_auth_json(r#"{"opencode":{"type":"api","key":"  "}}"#).is_err());
    }

    #[test]
    fn auth_path_candidates_include_xdg_local_share() {
        let candidates = auth_path_candidates();
        assert!(
            candidates
                .iter()
                .any(|p| p.ends_with(".local/share/opencode/auth.json")),
            "expected ~/.local/share candidate, got {candidates:?}"
        );
    }
}
