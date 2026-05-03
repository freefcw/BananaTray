use super::super::spec::CodeiumFamilySpec;
use crate::providers::{ProviderError, ProviderResult};
use log::debug;
use rusqlite::Connection;
use std::path::PathBuf;

pub(in crate::providers::codeium_family) fn cache_db_path_candidates(
    spec: &CodeiumFamilySpec,
) -> Vec<PathBuf> {
    // Windsurf / Antigravity 都是 VS Code 系 Electron 应用：
    // macOS 使用 ~/Library/Application Support，Linux 使用 XDG config。
    let config_relative = PathBuf::from(spec.cache_db_config_relative_path);
    let mut candidates = Vec::new();

    if cfg!(target_os = "macos") {
        if let Some(home) = dirs::home_dir() {
            push_unique(
                &mut candidates,
                home.join("Library")
                    .join("Application Support")
                    .join(&config_relative),
            );
        }
    } else if let Some(config_dir) = dirs::config_dir() {
        push_unique(&mut candidates, config_dir.join(&config_relative));
    }

    if let Some(home) = dirs::home_dir() {
        push_unique(
            &mut candidates,
            home.join("Library")
                .join("Application Support")
                .join(&config_relative),
        );
        push_unique(&mut candidates, home.join(".config").join(&config_relative));
    }

    candidates
}

fn push_unique(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.contains(&path) {
        paths.push(path);
    }
}

pub(in crate::providers::codeium_family) fn cache_db_path(
    spec: &CodeiumFamilySpec,
) -> ProviderResult<PathBuf> {
    let candidates = cache_db_path_candidates(spec);

    if candidates.is_empty() {
        return Err(ProviderError::unavailable(
            "cannot determine config directory",
        ));
    }

    for db_path in candidates {
        if db_path.exists() {
            debug!(
                target: "providers",
                "{} local cache DB: {}",
                spec.log_label,
                db_path.display()
            );
            return Ok(db_path);
        }
    }

    Err(ProviderError::unavailable(&format!(
        "{} local cache database not found",
        spec.log_label
    )))
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
