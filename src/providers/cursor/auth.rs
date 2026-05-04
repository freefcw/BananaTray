use crate::models::FailureAdvice;
use crate::providers::common::jwt;
use crate::providers::ProviderError;
use anyhow::Result;
use std::path::PathBuf;
use std::process::Command;

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
    build_db_path_candidates(dirs::config_dir(), dirs::home_dir())
}

/// 纯函数：根据给定的 config_dir 和 home_dir 构建候选路径列表。
///
/// 便于表驱动测试覆盖所有组合（macOS/Linux/XDG/无 home/去重），
/// 公开 API `db_path_candidates()` 传入真实 `dirs::*` 调用此函数。
fn build_db_path_candidates(
    config_dir: Option<PathBuf>,
    home_dir: Option<PathBuf>,
) -> Vec<PathBuf> {
    const CURSOR_DB_SUFFIX: &str = "Cursor/User/globalStorage/state.vscdb";
    let mut candidates = Vec::new();

    if let Some(dir) = config_dir {
        let path = dir.join(CURSOR_DB_SUFFIX);
        if !candidates.contains(&path) {
            candidates.push(path);
        }
    }

    if let Some(home) = home_dir {
        let xdg_path = home.join(".config").join(CURSOR_DB_SUFFIX);
        if !candidates.contains(&xdg_path) {
            candidates.push(xdg_path);
        }
    }

    candidates
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

    // ── build_db_path_candidates 表驱动测试 ──

    #[test]
    fn test_build_candidates_both_dirs_provided() {
        let config = Some(PathBuf::from("/custom/config"));
        let home = Some(PathBuf::from("/home/user"));
        let candidates = build_db_path_candidates(config, home);
        assert_eq!(candidates.len(), 2, "should have primary + XDG fallback");
        assert!(
            candidates[0].starts_with("/custom/config"),
            "primary should start with config_dir"
        );
        assert!(
            candidates[1].starts_with("/home/user/.config"),
            "fallback should start with home/.config"
        );
    }

    #[test]
    fn test_build_candidates_config_only() {
        let config = Some(PathBuf::from("/custom/config"));
        let candidates = build_db_path_candidates(config, None);
        assert_eq!(candidates.len(), 1, "should have only primary candidate");
        assert!(candidates[0].starts_with("/custom/config"));
    }

    #[test]
    fn test_build_candidates_home_only() {
        let home = Some(PathBuf::from("/home/user"));
        let candidates = build_db_path_candidates(None, home);
        assert_eq!(candidates.len(), 1, "should have only XDG fallback");
        assert!(candidates[0].starts_with("/home/user/.config"));
    }

    #[test]
    fn test_build_candidates_no_dirs() {
        let candidates = build_db_path_candidates(None, None);
        assert!(
            candidates.is_empty(),
            "should have no candidates when both dirs are None"
        );
    }

    #[test]
    fn test_build_candidates_dedup_linux_default() {
        // Linux 默认：config_dir == home/.config，应去重为 1 条
        let config = Some(PathBuf::from("/home/user/.config"));
        let home = Some(PathBuf::from("/home/user"));
        let candidates = build_db_path_candidates(config, home);
        assert_eq!(candidates.len(), 1, "Linux default should deduplicate to 1");
        assert!(candidates[0].starts_with("/home/user/.config"));
    }

    #[test]
    fn test_build_candidates_no_dedup_macos() {
        // macOS：config_dir != home/.config，应保留 2 条
        let config = Some(PathBuf::from("/Users/user/Library/Application Support"));
        let home = Some(PathBuf::from("/Users/user"));
        let candidates = build_db_path_candidates(config, home);
        assert_eq!(candidates.len(), 2, "macOS should keep both candidates");
    }

    #[test]
    fn test_build_candidates_all_end_with_cursor_suffix() {
        let config = Some(PathBuf::from("/custom/config"));
        let home = Some(PathBuf::from("/home/user"));
        let candidates = build_db_path_candidates(config, home);
        assert!(
            candidates
                .iter()
                .all(|p| p.ends_with("Cursor/User/globalStorage/state.vscdb")),
            "all candidates should end with Cursor suffix, got: {:?}",
            candidates
        );
    }

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
            candidates.first().map(|p| p.clone()),
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
