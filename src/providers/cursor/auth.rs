use crate::models::FailureAdvice;
#[cfg(target_os = "macos")]
use crate::providers::common::cli;
use crate::providers::common::config_paths;
use crate::providers::common::jwt;
use crate::providers::ProviderError;
#[cfg(target_os = "macos")]
use aes::Aes128;
use anyhow::Result;
#[cfg(target_os = "macos")]
use base64::{engine::general_purpose::STANDARD, Engine};
#[cfg(target_os = "macos")]
use cbc::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
#[cfg(target_os = "macos")]
use pbkdf2::pbkdf2_hmac;
use rusqlite::{Connection, OpenFlags};
#[cfg(target_os = "macos")]
use sha1::Sha1;
use std::path::{Path, PathBuf};
#[cfg(target_os = "macos")]
use std::time::Duration;

const CURSOR_DB_SUFFIX: &str = "Cursor/User/globalStorage/state.vscdb";
const CURSOR_SAFE_STORAGE_PREFIX: &str = "cursor-safe-storage:v1:";

#[cfg(target_os = "macos")]
const CURSOR_SAFE_STORAGE_KEYCHAIN_TIMEOUT: Duration = Duration::from_secs(5);
#[cfg(target_os = "macos")]
const MACOS_SAFE_STORAGE_VERSION: &[u8] = b"v10";
#[cfg(target_os = "macos")]
const MACOS_SAFE_STORAGE_SALT: &[u8] = b"saltysalt";
#[cfg(target_os = "macos")]
const MACOS_SAFE_STORAGE_IV: &[u8; 16] = b"                ";
#[cfg(target_os = "macos")]
const MACOS_SAFE_STORAGE_PBKDF2_ITERATIONS: u32 = 1_003;

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
        Ok(token) => {
            let token = token.trim();
            if token.is_empty() {
                Ok(None)
            } else {
                decode_access_token(token).map(Some)
            }
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(err) => Err(ProviderError::parse_failed(&format!(
            "cannot query Cursor access token from {}: {}",
            db_path.display(),
            err
        ))
        .into()),
    }
}

fn decode_access_token(token: &str) -> Result<String> {
    let Some(encoded) = token.strip_prefix(CURSOR_SAFE_STORAGE_PREFIX) else {
        return Ok(token.to_string());
    };

    #[cfg(target_os = "macos")]
    {
        decrypt_macos_safe_storage_token(encoded)
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = encoded;
        Err(ProviderError::auth_required(Some(FailureAdvice::LoginApp {
            app: "Cursor".to_string(),
        }))
        .into())
    }
}

#[cfg(target_os = "macos")]
fn decrypt_macos_safe_storage_token(encoded: &str) -> Result<String> {
    let keychain_key = read_cursor_safe_storage_key()?;
    decrypt_macos_safe_storage_payload(encoded, keychain_key.as_bytes())
}

/// 读取 Cursor/Electron safeStorage 使用的 macOS Keychain 密钥。
///
/// Cursor 将 `safeStorage.encryptString()` 的结果包在自己的前缀中，但底层仍是
/// Chromium macOS safe storage：Keychain 密钥经 PBKDF2-SHA1 派生 AES-128-CBC 密钥，
/// 密文以 `v10` 开头。这里只读 Keychain，不写入或缓存密钥。
#[cfg(target_os = "macos")]
fn read_cursor_safe_storage_key() -> Result<String> {
    // 新版 Cursor 使用 Cursor Key；旧版 Electron 构建可能仍使用 Chromium Key。
    const KEYCHAIN_ENTRIES: [(&str, &str); 3] = [
        ("Cursor Safe Storage", "Cursor Key"),
        ("Cursor Safe Storage", "Cursor"),
        ("Chromium Safe Storage", "Chromium Key"),
    ];

    for (service, account) in KEYCHAIN_ENTRIES {
        let output = cli::run_command_with_timeout(
            "/usr/bin/security",
            &["find-generic-password", "-s", service, "-a", account, "-w"],
            CURSOR_SAFE_STORAGE_KEYCHAIN_TIMEOUT,
        );
        let Ok(output) = output else {
            continue;
        };
        if !output.status.success() {
            continue;
        }

        let key = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !key.is_empty() {
            return Ok(key);
        }
    }

    Err(ProviderError::auth_required(Some(FailureAdvice::LoginApp {
        app: "Cursor".to_string(),
    }))
    .into())
}

#[cfg(target_os = "macos")]
fn decrypt_macos_safe_storage_payload(encoded: &str, keychain_key: &[u8]) -> Result<String> {
    type Aes128CbcDec = cbc::Decryptor<Aes128>;

    let encrypted = STANDARD
        .decode(encoded)
        .map_err(|_| ProviderError::parse_failed("invalid Cursor safe storage payload encoding"))?;
    if encrypted.len() < MACOS_SAFE_STORAGE_VERSION.len()
        || &encrypted[..MACOS_SAFE_STORAGE_VERSION.len()] != MACOS_SAFE_STORAGE_VERSION
    {
        return Err(ProviderError::parse_failed("unsupported Cursor safe storage version").into());
    }

    let mut derived_key = [0u8; 16];
    pbkdf2_hmac::<Sha1>(
        keychain_key,
        MACOS_SAFE_STORAGE_SALT,
        MACOS_SAFE_STORAGE_PBKDF2_ITERATIONS,
        &mut derived_key,
    );

    let mut ciphertext = encrypted[MACOS_SAFE_STORAGE_VERSION.len()..].to_vec();
    let plaintext = Aes128CbcDec::new_from_slices(&derived_key, MACOS_SAFE_STORAGE_IV)
        .map_err(|_| ProviderError::parse_failed("invalid Cursor safe storage key"))?
        .decrypt_padded_mut::<Pkcs7>(&mut ciphertext)
        .map_err(|_| {
            ProviderError::auth_required(Some(FailureAdvice::LoginApp {
                app: "Cursor".to_string(),
            }))
        })?;

    String::from_utf8(plaintext.to_vec())
        .map_err(|_| ProviderError::parse_failed("Cursor access token is not valid UTF-8").into())
}

pub(super) fn extract_user_id_from_jwt(token: &str) -> Result<String> {
    let login_required = || {
        ProviderError::session_expired(Some(FailureAdvice::LoginApp {
            app: "Cursor".to_string(),
        }))
    };
    let payload: serde_json::Value = jwt::decode_payload(token).map_err(|_| login_required())?;
    let sub = payload
        .get("sub")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(login_required)?;

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

    #[test]
    fn decode_access_token_leaves_legacy_plaintext_token_unchanged() {
        assert_eq!(decode_access_token("plain-token").unwrap(), "plain-token");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn decrypt_macos_safe_storage_payload_returns_original_token() {
        use aes::Aes128;
        use base64::Engine;
        use cbc::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyIvInit};

        type Aes128CbcEnc = cbc::Encryptor<Aes128>;
        let keychain_key = b"test-keychain-key";
        let mut derived_key = [0u8; 16];
        pbkdf2_hmac::<Sha1>(
            keychain_key,
            MACOS_SAFE_STORAGE_SALT,
            MACOS_SAFE_STORAGE_PBKDF2_ITERATIONS,
            &mut derived_key,
        );

        let token = "header.eyJzdWIiOiJ1c2VyXzEyMyJ9.signature";
        let mut plaintext = token.as_bytes().to_vec();
        let plaintext_len = plaintext.len();
        plaintext.resize(plaintext_len + 16, 0);
        let ciphertext = Aes128CbcEnc::new_from_slices(&derived_key, MACOS_SAFE_STORAGE_IV)
            .unwrap()
            .encrypt_padded_mut::<Pkcs7>(&mut plaintext, plaintext_len)
            .unwrap();
        let mut encrypted = MACOS_SAFE_STORAGE_VERSION.to_vec();
        encrypted.extend_from_slice(ciphertext);

        let decoded =
            decrypt_macos_safe_storage_payload(&STANDARD.encode(encrypted), keychain_key).unwrap();
        assert_eq!(decoded, token);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn decrypt_macos_safe_storage_payload_rejects_unknown_version() {
        let err = decrypt_macos_safe_storage_payload(
            &STANDARD.encode(b"v11encrypted"),
            b"test-keychain-key",
        )
        .unwrap_err();

        assert!(matches!(
            err.downcast_ref::<ProviderError>(),
            Some(ProviderError::ParseFailed { .. })
        ));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn decrypt_macos_safe_storage_payload_rejects_invalid_base64() {
        let err =
            decrypt_macos_safe_storage_payload("not-base64", b"test-keychain-key").unwrap_err();

        assert!(matches!(
            err.downcast_ref::<ProviderError>(),
            Some(ProviderError::ParseFailed { .. })
        ));
    }

    // ── extract_user_id_from_jwt 测试 ──

    #[test]
    fn test_extract_user_id_from_jwt_invalid_format() {
        let err = extract_user_id_from_jwt("badtoken").unwrap_err();
        assert!(
            matches!(
                err.downcast_ref::<ProviderError>(),
                Some(ProviderError::SessionExpired {
                    advice: Some(FailureAdvice::LoginApp { app }),
                }) if app == "Cursor"
            ),
            "invalid Cursor JWT should ask the user to log in, got: {err}"
        );
    }

    #[test]
    fn test_extract_user_id_from_jwt_valid() {
        use base64::Engine;
        let payload =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"sub":"user_123"}"#);
        let jwt = format!("header.{}.sig", payload);
        assert_eq!(extract_user_id_from_jwt(&jwt).unwrap(), "user_123");
    }

    #[test]
    fn test_extract_user_id_from_jwt_missing_sub_asks_user_to_login() {
        use base64::Engine;
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"email":"a"}"#);
        let jwt = format!("header.{}.sig", payload);
        let err = extract_user_id_from_jwt(&jwt).unwrap_err();
        assert!(matches!(
            err.downcast_ref::<ProviderError>(),
            Some(ProviderError::SessionExpired {
                advice: Some(FailureAdvice::LoginApp { app }),
            }) if app == "Cursor"
        ));
    }
}
