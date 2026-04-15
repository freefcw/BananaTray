use std::path::PathBuf;

pub fn app_config_dir() -> PathBuf {
    if cfg!(target_os = "macos") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("BananaTray");
        }
    } else if cfg!(target_os = "linux") {
        let config_dir = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .unwrap_or_else(|| PathBuf::from("."));
        return config_dir.join("bananatray");
    }

    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

pub fn settings_path() -> PathBuf {
    app_config_dir().join("settings.json")
}

pub fn custom_providers_dir() -> PathBuf {
    app_config_dir().join("providers")
}

pub fn custom_provider_path(filename: &str) -> PathBuf {
    custom_providers_dir().join(filename)
}

pub fn migrate_legacy_custom_providers_dir() -> std::io::Result<()> {
    let Some(legacy_dir) = legacy_custom_providers_dir() else {
        return Ok(());
    };

    let canonical_dir = custom_providers_dir();
    if same_dir(&legacy_dir, &canonical_dir) || !legacy_dir.exists() {
        return Ok(());
    }

    migrate_legacy_dir_contents(&legacy_dir, &canonical_dir)
}

fn legacy_custom_providers_dir() -> Option<PathBuf> {
    if !cfg!(target_os = "macos") {
        return None;
    }

    let home = std::env::var_os("HOME")?;
    Some(
        PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("bananatray")
            .join("providers"),
    )
}

fn migrate_legacy_dir_contents(
    legacy_dir: &std::path::Path,
    canonical_dir: &std::path::Path,
) -> std::io::Result<()> {
    if !legacy_dir.exists() {
        return Ok(());
    }

    std::fs::create_dir_all(canonical_dir)?;

    let mut entries: Vec<_> = std::fs::read_dir(legacy_dir)?.flatten().collect();
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let source = entry.path();
        let target = canonical_dir.join(entry.file_name());
        if target.exists() {
            log::warn!(
                target: "providers::custom",
                "skipping legacy custom provider migration because target already exists: {}",
                target.display()
            );
            continue;
        }

        std::fs::rename(&source, &target)?;
        log::info!(
            target: "providers::custom",
            "migrated legacy custom provider entry {} -> {}",
            source.display(),
            target.display()
        );
    }

    remove_dir_if_empty(legacy_dir)?;
    if let Some(parent) = legacy_dir.parent() {
        remove_dir_if_empty(parent)?;
    }

    Ok(())
}

fn remove_dir_if_empty(dir: &std::path::Path) -> std::io::Result<()> {
    let mut entries = std::fs::read_dir(dir)?;
    if entries.next().is_none() {
        std::fs::remove_dir(dir)?;
    }
    Ok(())
}

fn same_dir(a: &std::path::Path, b: &std::path::Path) -> bool {
    let (Ok(meta_a), Ok(meta_b)) = (std::fs::metadata(a), std::fs::metadata(b)) else {
        return false;
    };

    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;

        meta_a.dev() == meta_b.dev() && meta_a.ino() == meta_b.ino()
    }

    #[cfg(not(unix))]
    {
        let _ = (meta_a, meta_b);
        match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
            (Ok(ca), Ok(cb)) => ca == cb,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_legacy_dir_moves_files_to_canonical_dir() {
        let temp = tempfile::tempdir().unwrap();
        let legacy_dir = temp.path().join("legacy/providers");
        let canonical_dir = temp.path().join("canonical/providers");
        std::fs::create_dir_all(&legacy_dir).unwrap();
        std::fs::write(legacy_dir.join("a.yaml"), "id: a").unwrap();
        std::fs::write(legacy_dir.join("b.yml"), "id: b").unwrap();

        migrate_legacy_dir_contents(&legacy_dir, &canonical_dir).unwrap();

        assert!(canonical_dir.join("a.yaml").exists());
        assert!(canonical_dir.join("b.yml").exists());
        assert!(!legacy_dir.exists());
    }

    #[test]
    fn migrate_legacy_dir_keeps_existing_canonical_files() {
        let temp = tempfile::tempdir().unwrap();
        let legacy_dir = temp.path().join("legacy/providers");
        let canonical_dir = temp.path().join("canonical/providers");
        std::fs::create_dir_all(&legacy_dir).unwrap();
        std::fs::create_dir_all(&canonical_dir).unwrap();
        std::fs::write(legacy_dir.join("a.yaml"), "legacy").unwrap();
        std::fs::write(canonical_dir.join("a.yaml"), "canonical").unwrap();
        std::fs::write(legacy_dir.join("b.yaml"), "legacy-b").unwrap();

        migrate_legacy_dir_contents(&legacy_dir, &canonical_dir).unwrap();

        assert_eq!(
            std::fs::read_to_string(canonical_dir.join("a.yaml")).unwrap(),
            "canonical"
        );
        assert_eq!(
            std::fs::read_to_string(canonical_dir.join("b.yaml")).unwrap(),
            "legacy-b"
        );
        assert!(legacy_dir.join("a.yaml").exists());
    }

    #[test]
    fn migrate_legacy_dir_is_noop_when_source_missing() {
        let temp = tempfile::tempdir().unwrap();
        let legacy_dir = temp.path().join("missing/providers");
        let canonical_dir = temp.path().join("canonical/providers");

        migrate_legacy_dir_contents(&legacy_dir, &canonical_dir).unwrap();

        assert!(!canonical_dir.exists());
    }
}
