use super::parse_strategy::{CacheParseStrategy, ParseStrategy};
use super::spec::CodeiumFamilySpec;
use crate::models::RefreshData;
use crate::providers::ProviderError;
use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use log::debug;
use rusqlite::{Connection, OpenFlags};
use std::path::PathBuf;

pub fn is_available(spec: &CodeiumFamilySpec) -> bool {
    cache_db_path(spec).is_ok()
}

pub fn read_refresh_data(spec: &CodeiumFamilySpec) -> Result<RefreshData> {
    let db_path = cache_db_path(spec)?;
    let conn = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| {
            format!(
                "cannot open {} cache DB: {}",
                spec.log_label,
                db_path.display()
            )
        })?;

    let auth_status_json = query_auth_status_json(&conn, spec)?;
    let user_status_data = decode_user_status_payload(&auth_status_json)?;
    let strategy = CacheParseStrategy;
    let (quotas, email, plan_name) = strategy.parse(&user_status_data)?;

    Ok(RefreshData::with_account(quotas, email, plan_name))
}

fn cache_db_path(spec: &CodeiumFamilySpec) -> Result<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| ProviderError::unavailable("cannot determine home directory"))?;
    let db_path = home.join(spec.cache_db_relative_path);

    if !db_path.exists() {
        return Err(ProviderError::unavailable(&format!(
            "{} local cache database not found",
            spec.log_label
        ))
        .into());
    }

    debug!(
        target: "providers",
        "{} local cache DB: {}",
        spec.log_label,
        db_path.display()
    );
    Ok(db_path)
}

pub(super) fn query_auth_status_json(
    conn: &Connection,
    spec: &CodeiumFamilySpec,
) -> Result<String> {
    for key in spec.auth_status_key_candidates {
        match conn.query_row("SELECT value FROM ItemTable WHERE key = ?1", [key], |row| {
            row.get(0)
        }) {
            Ok(value) => return Ok(value),
            Err(rusqlite::Error::QueryReturnedNoRows) => continue,
            Err(e) => {
                return Err(
                    ProviderError::parse_failed(&format!("cannot query {}: {}", key, e)).into(),
                )
            }
        }
    }

    Err(ProviderError::parse_failed(&format!(
        "cannot find auth status key in local cache: {}",
        spec.auth_status_key_candidates.join(", ")
    ))
    .into())
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

    #[test]
    fn test_query_auth_status_json_uses_fallback_keys() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO ItemTable (key, value) VALUES (?1, ?2)",
            ["antigravityAuthStatus", "payload-json"],
        )
        .unwrap();

        let spec = CodeiumFamilySpec {
            kind: crate::models::ProviderKind::Windsurf,
            provider_id: "windsurf:api",
            display_name: "Windsurf",
            brand_name: "Codeium",
            icon_asset: "src/icons/provider-windsurf.svg",
            dashboard_url: "https://windsurf.com/",
            account_hint: "Windsurf account",
            source_label: "local api",
            log_label: "Windsurf",
            ide_name: "windsurf",
            unavailable_message: "Windsurf live source and local cache are both unavailable",
            cache_db_relative_path:
                "Library/Application Support/Windsurf/User/globalStorage/state.vscdb",
            auth_status_key_candidates: &["windsurfAuthStatus", "antigravityAuthStatus"],
            process_markers: &["--ide_name windsurf", "/windsurf/", "/windsurf.app/"],
        };

        let value = query_auth_status_json(&conn, &spec).unwrap();
        assert_eq!(value, "payload-json");
    }
}
