use super::super::spec::CodeiumFamilySpec;
use crate::providers::{ProviderError, ProviderResult};
use log::debug;
use rusqlite::Connection;
use std::path::PathBuf;

pub(in crate::providers::codeium_family) fn cache_db_path(
    spec: &CodeiumFamilySpec,
) -> ProviderResult<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| ProviderError::unavailable("cannot determine home directory"))?;
    let db_path = home.join(spec.cache_db_relative_path);

    if !db_path.exists() {
        return Err(ProviderError::unavailable(&format!(
            "{} local cache database not found",
            spec.log_label
        )));
    }

    debug!(
        target: "providers",
        "{} local cache DB: {}",
        spec.log_label,
        db_path.display()
    );
    Ok(db_path)
}

pub(in crate::providers::codeium_family) fn query_auth_status_json(
    conn: &Connection,
    spec: &CodeiumFamilySpec,
) -> ProviderResult<String> {
    for key in spec.auth_status_key_candidates {
        match conn.query_row("SELECT value FROM ItemTable WHERE key = ?1", [key], |row| {
            row.get(0)
        }) {
            Ok(value) => return Ok(value),
            Err(rusqlite::Error::QueryReturnedNoRows) => continue,
            Err(e) => {
                return Err(ProviderError::parse_failed(&format!(
                    "cannot query {}: {}",
                    key, e
                )))
            }
        }
    }

    Err(ProviderError::parse_failed(&format!(
        "cannot find auth status key in local cache: {}",
        spec.auth_status_key_candidates.join(", ")
    )))
}

pub(super) fn query_cached_plan_info(
    conn: &Connection,
    spec: &CodeiumFamilySpec,
) -> ProviderResult<String> {
    for key in spec.cached_plan_info_key_candidates {
        match conn.query_row("SELECT value FROM ItemTable WHERE key = ?1", [key], |row| {
            row.get(0)
        }) {
            Ok(value) => {
                debug!(
                    target: "providers",
                    "{} found cachedPlanInfo via key '{}'",
                    spec.log_label,
                    key
                );
                return Ok(value);
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => continue,
            Err(e) => {
                return Err(ProviderError::parse_failed(&format!(
                    "cannot query {}: {}",
                    key, e
                )))
            }
        }
    }

    Err(ProviderError::parse_failed(&format!(
        "cannot find cachedPlanInfo key in local cache: {}",
        spec.cached_plan_info_key_candidates.join(", ")
    )))
}
