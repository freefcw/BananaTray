use crate::models::FailureAdvice;
use crate::providers::common::config_paths;
use crate::providers::common::jwt;
use crate::providers::ProviderError;
use anyhow::Result;
use std::path::PathBuf;
use std::process::Command;

const CURSOR_DB_SUFFIX: &str = "Cursor/User/globalStorage/state.vscdb";

/// Cursor 数据库路径候选列表，跨平台解析。
///
/// 依赖 `dirs::config_dir()` 和 `dirs::home_dir()` 自动适配平台：
/// - macOS 主候选: `~/Library/Application Support/Cursor/User/globalStorage/state.vscdb`
/// - macOS fallback: `~/.config/Cursor/User/globalStorage/state.vscdb`
/// - Linux 主候选: `~/.config/Cursor/User/globalStorage/state.vscdb`（与 fallback 去重后仅一条）
///
/// 与 Copilot / Codeium family 一致：当 `XDG_CONFIG_HOME` 在 BananaTray 与 Cursor 进程间不一致时，
/// fallback 路径仍能找到数据库。
pub(super) fn db_path_candidates() -> Vec<PathBuf> {
    config_paths::config_dir_with_xdg_fallback(CURSOR_DB_SUFFIX)
}

/// 返回用于错误提示的数据库路径描述（使用 `~/` 前缀）。
///
/// 因为需要编译期常量字符串，这里用 `cfg!` 而非 `dirs::config_dir()`。
pub(super) fn db_path_display() -> &'static str {
    if cfg!(target_os = "macos") {
        "~/Library/Application Support/Cursor/User/globalStorage/state.vscdb"
    } else {
        "~/.config/Cursor/User/globalStorage/state.vscdb"
    }
}

pub(super) fn read_access_token() -> Result<String> {
    let candidates = db_path_candidates();
    let existing: Vec<_> = candidates.into_iter().filter(|p| p.exists()).collect();

    if existing.is_empty() {
        return Err(ProviderError::config_missing(db_path_display()).into());
    }

    let mut last_error = None;
    for db_path in &existing {
        let db_str = db_path.to_string_lossy();

        let output = match Command::new("sqlite3")
            .args([
                &*db_str,
                "SELECT value FROM ItemTable WHERE key = 'cursorAuth/accessToken'",
            ])
            .output()
        {
            Ok(o) => o,
            Err(_) => {
                last_error = Some(ProviderError::cli_not_found("sqlite3").into());
                break; // sqlite3 本身不可用，换候选也无意义
            }
        };

        if !output.status.success() {
            last_error = Some(
                ProviderError::fetch_failed_with_advice(FailureAdvice::CliExitFailed {
                    code: output.status.code().unwrap_or(-1),
                })
                .into(),
            );
            continue;
        }

        let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !token.is_empty() {
            return Ok(token);
        }
        // token 为空，尝试下一个候选
        last_error = Some(
            ProviderError::auth_required(Some(FailureAdvice::LoginApp {
                app: "Cursor".to_string(),
            }))
            .into(),
        );
    }

    // 所有候选都失败，返回最后一个有意义的错误
    Err(last_error.unwrap_or_else(|| ProviderError::config_missing(db_path_display()).into()))
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

    // ── db_path_candidates 集成测试 ──

    #[test]
    fn test_db_path_candidates_non_empty() {
        let candidates = db_path_candidates();
        assert!(
            !candidates.is_empty(),
            "should have at least one candidate on any platform"
        );
    }

    #[test]
    fn test_db_path_candidates_primary_is_dirs_config() {
        let candidates = db_path_candidates();
        let expected_primary =
            dirs::config_dir().map(|d| d.join("Cursor/User/globalStorage/state.vscdb"));
        assert_eq!(
            candidates.first().cloned(),
            expected_primary,
            "primary candidate should be dirs::config_dir()/Cursor/..."
        );
    }

    #[test]
    fn test_db_path_candidates_includes_xdg_fallback() {
        let candidates = db_path_candidates();
        let xdg_fallback = dirs::home_dir().map(|h| {
            h.join(".config")
                .join("Cursor/User/globalStorage/state.vscdb")
        });
        if let Some(expected) = xdg_fallback {
            assert!(
                candidates.contains(&expected),
                "candidates should include ~/.config/Cursor/... as XDG fallback"
            );
        }
    }

    #[test]
    fn test_db_path_candidates_no_duplicates() {
        let candidates = db_path_candidates();
        let mut seen = std::collections::HashSet::new();
        for c in &candidates {
            assert!(
                seen.insert(c.clone()),
                "duplicate path found: {}",
                c.display()
            );
        }
    }

    // ── db_path_display 测试 ──

    #[test]
    fn test_db_path_display_non_empty() {
        assert!(
            !db_path_display().is_empty(),
            "db_path_display should not be empty"
        );
    }

    #[test]
    fn test_db_path_display_contains_cursor_suffix() {
        assert!(
            db_path_display().contains("Cursor/User/globalStorage/state.vscdb"),
            "db_path_display should contain Cursor suffix, got: {}",
            db_path_display()
        );
    }

    // ── extract_user_id_from_jwt 测试 ──

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
