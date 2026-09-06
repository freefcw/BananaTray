use super::super::spec::CodeiumFamilySpec;
use crate::providers::common::config_paths;
use crate::providers::{ProviderError, ProviderResult};
use log::debug;
use rusqlite::Connection;
use std::path::PathBuf;

pub(crate) fn cache_db_path_candidates(spec: &CodeiumFamilySpec) -> Vec<PathBuf> {
    // Devin / Antigravity 都是 VS Code 系 Electron 应用：
    // macOS 使用 ~/Library/Application Support，Linux 使用 XDG config。
    // dirs::config_dir() 已自动适配平台，无需 cfg! 分支。
    let mut candidates =
        config_paths::config_dir_with_xdg_fallback(spec.cache_db_config_relative_path);

    // 品牌迁移兜底：主路径不存在时尝试旧路径（例如 Windsurf → Devin 后旧 data dir 仍可用）
    for fallback in spec.cache_db_fallback_paths {
        for path in config_paths::config_dir_with_xdg_fallback(fallback) {
            if !candidates.contains(&path) {
                candidates.push(path);
            }
        }
    }

    candidates
}

pub(crate) fn cache_db_path(spec: &CodeiumFamilySpec) -> ProviderResult<PathBuf> {
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

pub(crate) fn query_auth_status_json(
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

pub(crate) fn query_cached_plan_info(
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
            display_name: "Devin",
            brand_name: "Cognition",
            icon_asset: "src/icons/provider-devin-desktop.svg",
            dashboard_url: "",
            account_hint: "Devin account",
            source_label: "local cache",
            log_label: "Devin",
            ide_name: "windsurf",
            unavailable_message: "Devin local cache unavailable",
            cache_db_config_relative_path: "Devin/User/globalStorage/state.vscdb",
            cache_db_fallback_paths: &["Windsurf/User/globalStorage/state.vscdb"],
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
            candidates.first().cloned(),
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
        let relative_path = PathBuf::from(spec.cache_db_config_relative_path);

        assert!(
            candidates.iter().any(|path| path.ends_with(&relative_path)
                && path
                    .components()
                    .any(|component| component.as_os_str() == ".config")),
            "candidates should include ~/.config/ XDG fallback, got: {:?}",
            candidates
        );
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
    fn test_cache_db_path_candidates_ends_with_known_relative() {
        let spec = windsurf_spec();
        let candidates = cache_db_path_candidates(&spec);
        let all_relative: Vec<&str> = std::iter::once(spec.cache_db_config_relative_path)
            .chain(spec.cache_db_fallback_paths.iter().copied())
            .collect();
        assert!(
            candidates
                .iter()
                .all(|p| all_relative.iter().any(|rel| p.ends_with(rel))),
            "all candidates should end with primary or fallback relative path, got: {:?}",
            candidates
        );
    }
}
