use super::parse_strategy::{CacheParseStrategy, ParseStrategy};
use crate::models::RefreshData;
use crate::providers::ProviderError;
use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use log::debug;
use rusqlite::{Connection, OpenFlags};
use std::path::PathBuf;

const CACHE_DB_RELATIVE_PATH: &str =
    "Library/Application Support/Antigravity/User/globalStorage/state.vscdb";

pub fn is_available() -> bool {
    cache_db_path().is_ok()
}

pub fn read_refresh_data() -> Result<RefreshData> {
    let db_path = cache_db_path()?;
    let conn = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("cannot open Antigravity cache DB: {}", db_path.display()))?;

    let auth_status_json: String = conn
        .query_row(
            "SELECT value FROM ItemTable WHERE key = 'antigravityAuthStatus'",
            [],
            |row| row.get(0),
        )
        .map_err(|e| {
            ProviderError::parse_failed(&format!("cannot query antigravityAuthStatus: {}", e))
        })?;

    let user_status_data = decode_user_status_payload(&auth_status_json)?;
    let strategy = CacheParseStrategy;
    let (quotas, email, plan_name) = strategy.parse(&user_status_data)?;

    Ok(RefreshData::with_account(quotas, email, plan_name))
}

fn cache_db_path() -> Result<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| ProviderError::unavailable("cannot determine home directory"))?;
    let db_path = home.join(CACHE_DB_RELATIVE_PATH);

    if !db_path.exists() {
        return Err(
            ProviderError::unavailable("Antigravity local cache database not found").into(),
        );
    }

    debug!(target: "providers", "Antigravity local cache DB: {}", db_path.display());
    Ok(db_path)
}

fn decode_user_status_payload(auth_status_json: &str) -> Result<Vec<u8>> {
    let auth_status: serde_json::Value = serde_json::from_str(auth_status_json)
        .map_err(|e| ProviderError::parse_failed(&format!("invalid auth status JSON: {}", e)))?;

    let user_status_b64 = auth_status
        .get("userStatusProtoBinaryBase64")
        .and_then(|value| value.as_str())
        .ok_or_else(|| ProviderError::parse_failed("missing userStatusProtoBinaryBase64 field"))?;

    STANDARD.decode(user_status_b64).map_err(|e| {
        ProviderError::parse_failed(&format!("invalid user status base64: {}", e)).into()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_user_status_payload_success() {
        let payload = STANDARD.encode(b"proto-bytes");
        let json = format!(r#"{{"userStatusProtoBinaryBase64":"{}"}}"#, payload);

        let data = decode_user_status_payload(&json).unwrap();
        assert_eq!(data, b"proto-bytes");
    }

    #[test]
    fn test_decode_user_status_payload_missing_field() {
        let err = decode_user_status_payload(r#"{"other":"value"}"#).unwrap_err();
        let provider_err = ProviderError::classify(&err);
        assert!(matches!(provider_err, ProviderError::ParseFailed { .. }));
    }
}
