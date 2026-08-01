use crate::models::AppSettings;
use crate::platform::atomic_file::write_private_file_atomically;
use anyhow::{Context, Result};
use log::debug;
use std::fs;
use std::path::{Path, PathBuf};

pub fn load() -> Result<AppSettings> {
    load_from(&config_path())
}

/// 加载失败时备份疑似损坏的设置文件，返回备份文件路径。
///
/// 使用 `rename` 将原文件移出加载路径：既保留现场供人工恢复，
/// 又避免后续 `persist` 把默认值写回时彻底覆盖原始内容。
/// 文件不存在或备份失败时返回 `None`（已记录日志）。
pub fn backup_corrupt_file() -> Option<PathBuf> {
    backup_corrupt_file_at(&config_path())
}

fn backup_corrupt_file_at(path: &Path) -> Option<PathBuf> {
    if !path.exists() {
        return None;
    }
    let file_name = path.file_name()?.to_str()?;
    let epoch_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let backup_path = path.with_file_name(format!("{file_name}.corrupt-{epoch_secs}"));
    match fs::rename(path, &backup_path) {
        Ok(()) => {
            log::warn!(
                target: "settings",
                "backed up corrupt settings file to {}",
                backup_path.display()
            );
            Some(backup_path)
        }
        Err(err) => {
            log::warn!(
                target: "settings",
                "failed to back up corrupt settings file {}: {err}",
                path.display()
            );
            None
        }
    }
}

/// 将 AppSettings 持久化到磁盘。
///
/// 返回 `true` 表示成功，`false` 表示失败（已记录日志）。
/// 大多数调用点可忽略返回值（fire-and-forget），仅在需要区分
/// 成功/失败并给用户不同反馈时才检查（如 NewApiEffect::SaveProvider）。
pub fn persist(settings: &AppSettings) -> bool {
    match save(settings) {
        Ok(_) => true,
        Err(err) => {
            log::warn!(target: "settings", "failed to save settings: {err}");
            false
        }
    }
}

/// 原子写入设置文件。
///
/// 策略：先写入同目录的唯一私有临时文件，同步内容后再 `rename` 到目标路径。
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

    let settings: AppSettings = serde_json::from_str(&content)
        .with_context(|| format!("failed to deserialize settings from {}", path.display()))?;

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

    write_private_file_atomically(path, content.as_bytes())
        .with_context(|| format!("failed to atomically save settings at {}", path.display()))?;

    debug!(target: "settings", "settings saved (atomic) to {}", path.display());
    Ok(path.to_path_buf())
}

pub fn config_path() -> PathBuf {
    crate::platform::paths::settings_path()
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
        let settings = AppSettings {
            display: crate::models::DisplaySettings {
                theme: AppTheme::Light,
                ..Default::default()
            },
            system: crate::models::SystemSettings {
                refresh_interval_mins: 42,
                ..Default::default()
            },
            ..Default::default()
        };

        save_to(&settings, &path).unwrap();
        let loaded = load_from(&path).unwrap();

        assert_eq!(loaded.display.theme, AppTheme::Light);
        assert_eq!(loaded.system.refresh_interval_mins, 42);
    }

    #[test]
    fn atomic_write_no_tmp_left_behind() {
        let (_dir, path) = temp_settings_path();
        save_to(&AppSettings::default(), &path).unwrap();

        assert!(path.exists(), "target file should exist");
        let entries = fs::read_dir(path.parent().unwrap()).unwrap().count();
        assert_eq!(entries, 1, "temp file should be cleaned up after rename");
    }

    #[cfg(unix)]
    #[test]
    fn save_replaces_wide_permissions_with_private_file() {
        use std::os::unix::fs::PermissionsExt;

        let (_dir, path) = temp_settings_path();
        let legacy_tmp = path.parent().unwrap().join("settings.json.tmp");
        fs::write(&legacy_tmp, b"legacy temp").unwrap();
        fs::set_permissions(&legacy_tmp, fs::Permissions::from_mode(0o644)).unwrap();

        save_to(&AppSettings::default(), &path).unwrap();

        let mode = fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
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

        assert_eq!(settings.display.theme, AppSettings::default().display.theme);
        assert_eq!(
            settings.system.refresh_interval_mins,
            AppSettings::default().system.refresh_interval_mins
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
    fn backup_corrupt_file_renames_and_preserves_content() {
        let (_dir, path) = temp_settings_path();
        fs::write(&path, "not valid json {{{").unwrap();

        let backup = backup_corrupt_file_at(&path).expect("backup should succeed");

        assert!(!path.exists(), "original file should be moved away");
        assert_eq!(fs::read_to_string(&backup).unwrap(), "not valid json {{{");
        let backup_name = backup.file_name().unwrap().to_str().unwrap();
        assert!(
            backup_name.starts_with("settings.json.corrupt-"),
            "unexpected backup name: {backup_name}"
        );
    }

    #[test]
    fn backup_corrupt_file_missing_returns_none() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("nonexistent.json");

        assert!(backup_corrupt_file_at(&path).is_none());
    }

    #[test]
    fn save_overwrites_existing_file() {
        let (_dir, path) = temp_settings_path();

        let s1 = AppSettings {
            system: crate::models::SystemSettings {
                refresh_interval_mins: 1,
                ..Default::default()
            },
            ..Default::default()
        };
        save_to(&s1, &path).unwrap();

        let s2 = AppSettings {
            system: crate::models::SystemSettings {
                refresh_interval_mins: 99,
                ..Default::default()
            },
            ..Default::default()
        };
        save_to(&s2, &path).unwrap();

        let loaded = load_from(&path).unwrap();
        assert_eq!(loaded.system.refresh_interval_mins, 99);
    }
}
