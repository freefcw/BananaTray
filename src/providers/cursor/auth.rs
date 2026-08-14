use crate::models::FailureAdvice;
use crate::providers::common::config_paths;
use crate::providers::common::jwt;
use crate::providers::ProviderError;
use anyhow::Result;
use rusqlite::{Connection, OpenFlags};
use std::path::{Path, PathBuf};

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
        match read_access_token_from_db(db_path) {
            Ok(Some(token)) => return Ok(token),
            Ok(None) => {
                last_error = Some(
                    ProviderError::auth_required(Some(FailureAdvice::LoginApp {
                        app: "Cursor".to_string(),
                    }))
                    .into(),
                );
            }
            Err(err) => last_error = Some(err),
        }
    }

    // 所有候选都失败，返回最后一个有意义的错误
    Err(last_error.unwrap_or_else(|| ProviderError::config_missing(db_path_display()).into()))
}

fn read_access_token_from_db(db_path: &Path) -> Result<Option<String>> {
    let conn =
        Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|err| {
            ProviderError::fetch_failed(&format!(
                "cannot open Cursor state database at {}: {}",
                db_path.display(),
                err
            ))
        })?;

    match conn.query_row(
        "SELECT value FROM ItemTable WHERE key = 'cursorAuth/accessToken'",
        [],
        |row| row.get::<_, String>(0),
    ) {
        Ok(token) => Ok((!token.trim().is_empty()).then(|| token.trim().to_string())),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(err) => Err(ProviderError::parse_failed(&format!(
            "cannot query Cursor access token from {}: {}",
            db_path.display(),
            err
        ))
        .into()),
    }
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

    fn create_cursor_db() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.vscdb");
        let conn = Connection::open(path).unwrap();
        conn.execute("CREATE TABLE ItemTable (key TEXT UNIQUE, value BLOB)", [])
            .unwrap();
        dir
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

    #[test]
    fn read_access_token_from_db_returns_stored_token() {
        let dir = create_cursor_db();
        let path = dir.path().join("state.vscdb");
        let conn = Connection::open(&path).unwrap();
        conn.execute(
            "INSERT INTO ItemTable (key, value) VALUES (?1, ?2)",
            ("cursorAuth/accessToken", "cursor-token"),
        )
        .unwrap();
        drop(conn);

        assert_eq!(
            read_access_token_from_db(&path).unwrap().as_deref(),
            Some("cursor-token")
        );
    }

    #[test]
    fn read_access_token_from_db_returns_none_when_token_is_missing() {
        let dir = create_cursor_db();
        let path = dir.path().join("state.vscdb");

        assert_eq!(read_access_token_from_db(&path).unwrap(), None);
    }

    #[test]
    fn read_access_token_from_db_classifies_invalid_schema_as_parse_failed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.vscdb");
        Connection::open(&path).unwrap();

        let err = read_access_token_from_db(&path).unwrap_err();
        assert!(matches!(
            err.downcast_ref::<ProviderError>(),
            Some(ProviderError::ParseFailed { .. })
        ));
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
