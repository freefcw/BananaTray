use crate::models::AppSettings;
use anyhow::{Context, Result};
use log::debug;
use std::fs;
use std::path::{Path, PathBuf};

pub fn load() -> Result<AppSettings> {
    load_from(&config_path())
}

/// 原子写入设置文件。
///
/// 策略：先写入同目录的临时文件，再 `rename` 到目标路径。
/// `rename` 在同一文件系统上是原子操作，即使进程在写入过程中崩溃，
/// 目标文件也不会处于半写状态（要么是旧内容，要么是完整的新内容）。
pub fn save(settings: &AppSettings) -> Result<PathBuf> {
    let path = config_path();
    save_to(settings, &path)
}

fn load_from(path: &Path) -> Result<AppSettings> {
    debug!(target: "settings", "loading settings from {}", path.display());

    if !path.exists() {
        debug!(target: "settings", "settings file not found, using defaults");
        return Ok(AppSettings::default());
    }

    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read settings file at {}", path.display()))?;

    let settings = serde_json::from_str::<AppSettings>(&content)
        .with_context(|| format!("failed to parse settings file at {}", path.display()))?;

    debug!(target: "settings", "loaded settings from {}", path.display());
    Ok(settings)
}

fn save_to(settings: &AppSettings, path: &Path) -> Result<PathBuf> {
    debug!(target: "settings", "saving settings to {}", path.display());

    let parent = path
        .parent()
        .context("settings path has no parent directory")?;

    fs::create_dir_all(parent).with_context(|| {
        format!(
            "failed to create settings directory at {}",
            parent.display()
        )
    })?;

    let content = serde_json::to_string_pretty(settings)?;

    // 写入同目录临时文件（确保与目标在同一文件系统，rename 才是原子的）
    let tmp_path = parent.join("settings.json.tmp");
    fs::write(&tmp_path, &content).with_context(|| {
        format!(
            "failed to write temp settings file at {}",
            tmp_path.display()
        )
    })?;

    // 原子替换：rename 在同一文件系统上是原子操作
    fs::rename(&tmp_path, path).with_context(|| {
        format!(
            "failed to rename temp file {} -> {}",
            tmp_path.display(),
            path.display()
        )
    })?;

    debug!(target: "settings", "settings saved (atomic) to {}", path.display());
    Ok(path.to_path_buf())
}

pub fn config_path() -> PathBuf {
    if cfg!(target_os = "macos") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("BananaTray")
                .join("settings.json");
        }
    } else if cfg!(target_os = "linux") {
        // XDG Base Directory: $XDG_CONFIG_HOME 或 ~/.config
        let config_dir = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .unwrap_or_else(|| PathBuf::from("."));
        return config_dir.join("bananatray").join("settings.json");
    }

    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("settings.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::AppTheme;

    fn temp_settings_path() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("settings.json");
        (dir, path)
    }

    #[test]
    fn save_load_round_trip() {
        let (_dir, path) = temp_settings_path();
        let mut settings = AppSettings::default();
        settings.theme = AppTheme::Light;
        settings.refresh_interval_mins = 42;

        save_to(&settings, &path).unwrap();
        let loaded = load_from(&path).unwrap();

        assert_eq!(loaded.theme, AppTheme::Light);
        assert_eq!(loaded.refresh_interval_mins, 42);
    }

    #[test]
    fn atomic_write_no_tmp_left_behind() {
        let (_dir, path) = temp_settings_path();
        let parent = path.parent().unwrap();
        let tmp_path = parent.join("settings.json.tmp");

        save_to(&AppSettings::default(), &path).unwrap();

        assert!(path.exists(), "target file should exist");
        assert!(
            !tmp_path.exists(),
            "tmp file should be cleaned up after rename"
        );
    }

    #[test]
    fn save_creates_parent_directories() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("nested").join("deep").join("settings.json");

        save_to(&AppSettings::default(), &path).unwrap();

        assert!(path.exists());
    }

    #[test]
    fn load_missing_file_returns_defaults() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("nonexistent.json");

        let settings = load_from(&path).unwrap();

        assert_eq!(settings.theme, AppSettings::default().theme);
        assert_eq!(
            settings.refresh_interval_mins,
            AppSettings::default().refresh_interval_mins
        );
    }

    #[test]
    fn load_corrupt_file_returns_error() {
        let (_dir, path) = temp_settings_path();
        fs::write(&path, "not valid json {{{").unwrap();

        let result = load_from(&path);

        assert!(result.is_err());
    }

    #[test]
    fn save_overwrites_existing_file() {
        let (_dir, path) = temp_settings_path();

        let mut s1 = AppSettings::default();
        s1.refresh_interval_mins = 1;
        save_to(&s1, &path).unwrap();

        let mut s2 = AppSettings::default();
        s2.refresh_interval_mins = 99;
        save_to(&s2, &path).unwrap();

        let loaded = load_from(&path).unwrap();
        assert_eq!(loaded.refresh_interval_mins, 99);
    }
}
