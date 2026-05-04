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
    // dirs::config_dir() 已自动适配平台，无需 cfg! 分支。
    let config_relative = PathBuf::from(spec.cache_db_config_relative_path);
    let mut candidates = Vec::new();

    // 主候选：dirs::config_dir() 解析的平台标准路径
    if let Some(config_dir) = dirs::config_dir() {
        push_unique(&mut candidates, config_dir.join(&config_relative));
    }

    // Fallback：当 XDG_CONFIG_HOME 非默认时，应用可能仍在 ~/.config/ 下留有数据。
    // macOS 上此路径与主候选不同，会被 push_unique 保留；
    // Linux 上此路径与主候选相同，会被 push_unique 去重。
    if let Some(home) = dirs::home_dir() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ProviderKind;

    fn windsurf_spec() -> CodeiumFamilySpec {
        CodeiumFamilySpec {
            kind: ProviderKind::Windsurf,
            provider_id: "windsurf:api",
            display_name: "Windsurf",
            brand_name: "Windsurf",
            icon_asset: "src/icons/provider-windsurf.svg",
            dashboard_url: "",
            account_hint: "Windsurf account",
            source_label: "local cache",
            log_label: "Windsurf",
            ide_name: "windsurf",
            unavailable_message: "Windsurf local cache unavailable",
            cache_db_config_relative_path: "Windsurf/User/globalStorage/state.vscdb",
            auth_status_key_candidates: &["windsurfAuthStatus"],
            process_markers: &[],
            cached_plan_info_key_candidates: &[],
            cache_max_age_secs: 0,
        }
    }

    #[test]
    fn test_cache_db_path_candidates_non_empty() {
        let candidates = cache_db_path_candidates(&windsurf_spec());
        assert!(
            !candidates.is_empty(),
            "should have at least one candidate path"
        );
    }

    #[test]
    fn test_cache_db_path_candidates_primary_is_dirs_config() {
        let spec = windsurf_spec();
        let candidates = cache_db_path_candidates(&spec);
        let expected_primary =
            dirs::config_dir().map(|d| d.join(spec.cache_db_config_relative_path));
        assert_eq!(
            candidates.first().map(|p| p.clone()),
            expected_primary,
            "primary candidate should be dirs::config_dir()/{}, got: {:?}",
            spec.cache_db_config_relative_path,
            candidates
        );
    }

    #[test]
    fn test_cache_db_path_candidates_includes_xdg_fallback() {
        let spec = windsurf_spec();
        let candidates = cache_db_path_candidates(&spec);
        let xdg_fallback =
            dirs::home_dir().map(|h| h.join(".config").join(spec.cache_db_config_relative_path));
        if let Some(expected) = xdg_fallback {
            assert!(
                candidates.contains(&expected),
                "candidates should include ~/.config/ XDG fallback, got: {:?}",
                candidates
            );
        }
    }

    #[test]
    fn test_cache_db_path_candidates_no_duplicates() {
        let candidates = cache_db_path_candidates(&windsurf_spec());
        let mut seen = std::collections::HashSet::new();
        for c in &candidates {
            assert!(
                seen.insert(c.clone()),
                "duplicate path found: {}",
                c.display()
            );
        }
    }

    #[test]
    fn test_cache_db_path_candidates_ends_with_spec_relative() {
        let spec = windsurf_spec();
        let candidates = cache_db_path_candidates(&spec);
        assert!(
            candidates
                .iter()
                .all(|p| p.ends_with(&spec.cache_db_config_relative_path)),
            "all candidates should end with the spec-relative path, got: {:?}",
            candidates
        );
    }

    #[test]
    fn test_push_unique_adds_new_path() {
        let mut paths = vec![PathBuf::from("/a")];
        push_unique(&mut paths, PathBuf::from("/b"));
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[1], PathBuf::from("/b"));
    }

    #[test]
    fn test_push_unique_skips_duplicate() {
        let mut paths = vec![PathBuf::from("/a")];
        push_unique(&mut paths, PathBuf::from("/a"));
        assert_eq!(paths.len(), 1, "duplicate should be skipped");
    }
}
