use crate::models::AppSettings;
use anyhow::{Context, Result};
use log::debug;
use std::fs;
use std::path::{Path, PathBuf};

pub fn load() -> Result<AppSettings> {
    load_from(&config_path())
}

/// 将 AppSettings 持久化到磁盘。
///
/// 返回 `true` 表示成功，`false` 表示失败（已记录日志）。
/// 大多数调用点可忽略返回值（fire-and-forget），仅在需要区分
/// 成功/失败并给用户不同反馈时才检查（如 SaveNewApiProvider）。
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
