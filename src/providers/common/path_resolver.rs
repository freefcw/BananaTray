//! Shared CLI path resolution for provider command execution.
//!
//! macOS GUI apps usually inherit a minimal PATH, so CLI-based providers need
//! the same PATH enrichment whether they run through `std::process::Command`
//! or a PTY.

use std::path::{Path, PathBuf};

const HOME_BIN_SUFFIXES: &[&str] = &[
    ".local/bin",
    ".bun/bin",
    ".cargo/bin",
    ".npm-global/bin",
    ".amp/bin",
];

const SYSTEM_BIN_DIRS: &[&str] = &[
    "/opt/homebrew/bin",
    "/opt/homebrew/sbin",
    "/usr/local/bin",
    "/usr/local/sbin",
    "/usr/bin",
];

pub(crate) fn enriched_path() -> String {
    let current = std::env::var("PATH").unwrap_or_default();
    enrich_path(&current)
}

pub(crate) fn enrich_path(path: &str) -> String {
    let mut components: Vec<String> = path
        .split(':')
        .filter(|component| !component.is_empty())
        .map(str::to_string)
        .collect();

    for candidate in candidate_dirs().into_iter().rev() {
        if !components.iter().any(|component| component == &candidate)
            && Path::new(&candidate).exists()
        {
            components.insert(0, candidate);
        }
    }

    components.join(":")
}

pub(crate) fn locate_executable(binary: &str) -> Option<String> {
    let path = Path::new(binary);
    if path.is_absolute() && is_executable_file(path) {
        return Some(binary.to_string());
    }

    if let Ok(path) = which::which(binary) {
        return Some(path.to_string_lossy().to_string());
    }

    locate_in_dirs(binary, &candidate_dirs())
}

pub(crate) fn locate_in_dirs(binary: &str, dirs: &[String]) -> Option<String> {
    dirs.iter()
        .map(|base| PathBuf::from(base).join(binary))
        .find(|path| is_executable_file(path))
        .map(|path| path.to_string_lossy().to_string())
}

fn candidate_dirs() -> Vec<String> {
    let mut dirs = Vec::new();

    if let Ok(home) = std::env::var("HOME") {
        if !home.is_empty() {
            dirs.extend(
                HOME_BIN_SUFFIXES
                    .iter()
                    .map(|suffix| format!("{}/{}", home, suffix)),
            );
        }
    }

    dirs.extend(SYSTEM_BIN_DIRS.iter().map(|dir| (*dir).to_string()));
    dirs
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }

    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn make_executable(path: &Path) {
        use std::os::unix::fs::PermissionsExt;

        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
            .expect("chmod fake binary");
    }

    #[cfg(not(unix))]
    fn make_executable(_path: &Path) {}

    #[test]
    fn enrich_path_keeps_existing_path() {
        let result = enrich_path("/usr/bin");
        assert!(result.split(':').any(|component| component == "/usr/bin"));
    }

    #[test]
    fn locate_in_dirs_returns_match_from_injected_dir() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let fake_binary = tmp.path().join("codex_test_fake");
        std::fs::write(&fake_binary, b"#!/bin/sh\nexit 0\n").expect("write fake");
        make_executable(&fake_binary);

        let dirs = vec![tmp.path().to_string_lossy().to_string()];
        let found = locate_in_dirs("codex_test_fake", &dirs);
        assert_eq!(found.as_deref(), fake_binary.to_str());
    }

    #[test]
    fn locate_in_dirs_returns_none_when_absent() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let dirs = vec![tmp.path().to_string_lossy().to_string()];
        assert!(locate_in_dirs("definitely_not_present_bananatray", &dirs).is_none());
    }

    #[test]
    fn locate_in_dirs_picks_first_match() {
        let first = tempfile::tempdir().expect("tmp1");
        let second = tempfile::tempdir().expect("tmp2");
        let first_hit = first.path().join("dup_bin");
        let second_hit = second.path().join("dup_bin");
        std::fs::write(&first_hit, b"first").expect("write first");
        std::fs::write(&second_hit, b"second").expect("write second");
        make_executable(&first_hit);
        make_executable(&second_hit);

        let dirs = vec![
            first.path().to_string_lossy().to_string(),
            second.path().to_string_lossy().to_string(),
        ];
        assert_eq!(
            locate_in_dirs("dup_bin", &dirs).as_deref(),
            first_hit.to_str()
        );
    }
}
