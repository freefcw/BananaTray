use crate::models::{
    AppSettings, DisplaySettings, LoggingSettings, NotificationSettings, ProviderConfig,
    SystemSettings,
};
use crate::platform::atomic_file::write_private_file_atomically;
use anyhow::{Context, Result};
use log::debug;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// settings.json 当前持久化版本。
///
/// 该 DTO 刻意与运行时 `AppSettings` 分离：未来磁盘 schema 迁移应发生在此边界，
/// 领域模型不再直接承担顶层文件格式契约。字段形状保持与既有 settings.json 一致。
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct PersistedAppSettingsV1 {
    system: SystemSettings,
    notification: NotificationSettings,
    display: DisplaySettings,
    logging: LoggingSettings,
    provider: ProviderConfig,
}

impl From<PersistedAppSettingsV1> for AppSettings {
    fn from(value: PersistedAppSettingsV1) -> Self {
        Self {
            system: value.system,
            notification: value.notification,
            display: value.display,
            logging: value.logging,
            provider: value.provider,
        }
    }
}

impl From<&AppSettings> for PersistedAppSettingsV1 {
    fn from(value: &AppSettings) -> Self {
        Self {
            system: value.system.clone(),
            notification: value.notification.clone(),
            display: value.display.clone(),
            logging: value.logging.clone(),
            provider: value.provider.clone(),
        }
    }
}

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

    let settings: PersistedAppSettingsV1 = serde_json::from_str(&content)
        .with_context(|| format!("failed to deserialize settings from {}", path.display()))?;

    debug!(target: "settings", "loaded settings from {}", path.display());
    Ok(settings.into())
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

    let persisted = PersistedAppSettingsV1::from(settings);
    let mut serialized = serde_json::to_value(persisted)?;
    if let Ok(existing_content) = fs::read_to_string(path) {
        if let Ok(mut existing) = serde_json::from_str::<serde_json::Value>(&existing_content) {
            // `linux_last_position` 是已知但可省略的字段。用户将它重置为默认值后，
            // 只移除这个字段，保留新版可能写入 `tray_popup` 的其他成员。
            if settings.display.tray_popup.linux_last_position.is_none() {
                if let Some(display) = existing
                    .get_mut("display")
                    .and_then(serde_json::Value::as_object_mut)
                {
                    let tray_popup_is_empty = display
                        .get_mut("tray_popup")
                        .and_then(serde_json::Value::as_object_mut)
                        .is_some_and(|tray_popup| {
                            tray_popup.remove("linux_last_position");
                            tray_popup.is_empty()
                        });
                    if tray_popup_is_empty {
                        display.remove("tray_popup");
                    }
                }
            }
            replace_dynamic_provider_maps(&mut existing, &serialized);
            serialized = merge_preserving_unknown_fields(existing, serialized);
        }
    }
    let content = serde_json::to_string_pretty(&serialized)?;

    write_private_file_atomically(path, content.as_bytes())
        .with_context(|| format!("failed to atomically save settings at {}", path.display()))?;

    debug!(target: "settings", "settings saved (atomic) to {}", path.display());
    Ok(path.to_path_buf())
}

/// 用当前已知字段覆盖旧文档，同时保留新版本添加的未知字段。
///
/// 这样旧版应用读取并保存由新版生成的兼容 JSON 时，不会静默删除自己不认识的
/// 顶层或嵌套配置。已知字段始终以当前 `AppSettings` 为准。
fn merge_preserving_unknown_fields(
    mut existing: serde_json::Value,
    current: serde_json::Value,
) -> serde_json::Value {
    let (Some(existing), Some(current)) = (existing.as_object_mut(), current.as_object()) else {
        return current;
    };

    for (key, current_value) in current {
        let merged = existing
            .remove(key)
            .map(|existing_value| {
                merge_preserving_unknown_fields(existing_value, current_value.clone())
            })
            .unwrap_or_else(|| current_value.clone());
        existing.insert(key.clone(), merged);
    }

    serde_json::Value::Object(std::mem::take(existing))
}

/// 动态 map 的键是用户数据，不是可向前兼容的 schema 字段。
///
/// 保存时以当前领域状态整体替换这些 map，确保删除 credential、provider 状态或
/// 隐藏配额后不会被通用的未知字段合并重新带回。
fn replace_dynamic_provider_maps(existing: &mut serde_json::Value, current: &serde_json::Value) {
    const DYNAMIC_MAP_FIELDS: [&str; 3] = ["credentials", "enabled_providers", "hidden_quotas"];

    let Some(existing_provider) = existing
        .get_mut("provider")
        .and_then(serde_json::Value::as_object_mut)
    else {
        return;
    };
    let Some(current_provider) = current
        .get("provider")
        .and_then(serde_json::Value::as_object)
    else {
        return;
    };

    for field in DYNAMIC_MAP_FIELDS {
        if let Some(current_value) = current_provider.get(field) {
            existing_provider.insert(field.to_string(), current_value.clone());
        }
    }
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
    fn current_format_round_trips_through_persistence_dto() {
        let (_dir, path) = temp_settings_path();
        save_to(&AppSettings::default(), &path).unwrap();

        let restored = load_from(&path).unwrap();

        assert_eq!(
            restored.display.tray_icon_style,
            crate::models::TrayIconStyle::default()
        );
        assert_eq!(restored.system.refresh_interval_mins, 5);
    }

    #[test]
    fn empty_object_deserializes_to_domain_defaults() {
        let (_dir, path) = temp_settings_path();
        fs::write(&path, "{}").unwrap();

        let restored = load_from(&path).unwrap();

        assert!(restored.system.auto_hide_window);
        assert_eq!(
            restored.system.refresh_interval_mins,
            SystemSettings::DEFAULT_REFRESH_INTERVAL_MINS
        );
        assert_eq!(
            restored.system.global_hotkey,
            SystemSettings::DEFAULT_GLOBAL_HOTKEY
        );
        assert!(restored.notification.session_quota_notifications);
        assert_eq!(restored.display.theme, AppTheme::Dark);
        assert!(restored.display.show_overview);
        assert_eq!(
            restored.logging.max_bytes,
            LoggingSettings::default().max_bytes
        );
    }

    #[test]
    fn partial_document_fills_domain_defaults() {
        let (_dir, path) = temp_settings_path();
        fs::write(&path, r#"{"system": {"refresh_interval_mins": 42}}"#).unwrap();

        let restored = load_from(&path).unwrap();

        assert_eq!(restored.system.refresh_interval_mins, 42);
        assert!(restored.system.auto_hide_window);
        assert_eq!(restored.display.theme, AppTheme::Dark);
    }

    #[test]
    fn missing_global_hotkey_uses_domain_default() {
        let (_dir, path) = temp_settings_path();
        fs::write(
            &path,
            r#"{"system":{"auto_hide_window":true,"start_at_login":false,"refresh_interval_mins":5}}"#,
        )
        .unwrap();

        let restored = load_from(&path).unwrap();

        assert_eq!(
            restored.system.global_hotkey,
            SystemSettings::DEFAULT_GLOBAL_HOTKEY
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

    #[test]
    fn save_preserves_unknown_fields_from_newer_schema() {
        let (_dir, path) = temp_settings_path();
        fs::write(
            &path,
            serde_json::to_vec_pretty(&serde_json::json!({
                "system": {
                    "refresh_interval_mins": 42,
                    "future_system_option": "keep-me"
                },
                "future_section": {
                    "enabled": true
                }
            }))
            .unwrap(),
        )
        .unwrap();

        let mut settings = load_from(&path).unwrap();
        settings.system.refresh_interval_mins = 15;
        save_to(&settings, &path).unwrap();

        let saved: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved["system"]["refresh_interval_mins"], 15);
        assert_eq!(saved["system"]["future_system_option"], "keep-me");
        assert_eq!(saved["future_section"]["enabled"], true);
    }

    #[test]
    fn save_removes_known_optional_field_after_reset_to_default() {
        let (_dir, path) = temp_settings_path();
        let mut settings = AppSettings::default();
        settings.display.tray_popup.linux_last_position =
            Some(crate::models::SavedWindowPosition { x: 12.0, y: 34.0 });
        save_to(&settings, &path).unwrap();

        settings.display.tray_popup.linux_last_position = None;
        save_to(&settings, &path).unwrap();

        let saved: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert!(
            saved["display"].get("tray_popup").is_none(),
            "已知可选字段重置为默认值后不应被兼容性合并带回"
        );
    }

    #[test]
    fn save_persists_enabled_provider_entry_removal() {
        let (_dir, path) = temp_settings_path();
        let mut settings = AppSettings::default();
        let provider_id = crate::models::ProviderId::Custom("removed:newapi".to_string());
        settings.provider.set_enabled(&provider_id, true);
        save_to(&settings, &path).unwrap();

        settings.provider.remove_enabled_record(&provider_id);
        save_to(&settings, &path).unwrap();

        let saved: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert!(
            saved["provider"]["enabled_providers"]
                .get(provider_id.id_key())
                .is_none(),
            "已删除的动态 provider 启用记录不应被兼容性合并带回"
        );
    }

    #[test]
    fn save_persists_provider_credential_removal() {
        let (_dir, path) = temp_settings_path();
        let mut settings = AppSettings::default();
        settings
            .provider
            .credentials
            .set_credential("removed_token", "secret".to_string());
        save_to(&settings, &path).unwrap();

        settings
            .provider
            .credentials
            .remove_credential("removed_token");
        save_to(&settings, &path).unwrap();

        let saved: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert!(
            saved["provider"]["credentials"]
                .get("removed_token")
                .is_none(),
            "已删除的动态 credential 不应被兼容性合并带回"
        );
    }

    #[test]
    fn save_persists_hidden_quota_entry_removal() {
        let (_dir, path) = temp_settings_path();
        let mut settings = AppSettings::default();
        let provider_id = crate::models::ProviderId::Custom("removed:newapi".to_string());
        settings
            .provider
            .hidden_quotas
            .insert(provider_id.id_key(), ["session".to_string()].into());
        save_to(&settings, &path).unwrap();

        settings
            .provider
            .hidden_quotas
            .remove(&provider_id.id_key());
        save_to(&settings, &path).unwrap();

        let saved: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert!(
            saved["provider"]["hidden_quotas"]
                .get(provider_id.id_key())
                .is_none(),
            "已删除的动态 hidden_quotas 条目不应被兼容性合并带回"
        );
    }

    #[test]
    fn save_reset_position_preserves_unknown_tray_popup_fields() {
        let (_dir, path) = temp_settings_path();
        fs::write(
            &path,
            serde_json::to_vec_pretty(&serde_json::json!({
                "display": {
                    "tray_popup": {
                        "linux_last_position": {"x": 12.0, "y": 34.0},
                        "future_anchor_policy": "screen-edge"
                    }
                }
            }))
            .unwrap(),
        )
        .unwrap();

        let mut settings = load_from(&path).unwrap();
        settings.display.tray_popup.linux_last_position = None;
        save_to(&settings, &path).unwrap();

        let saved: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert!(
            saved["display"]["tray_popup"]
                .get("linux_last_position")
                .is_none(),
            "已重置的窗口位置不应被兼容性合并带回"
        );
        assert_eq!(
            saved["display"]["tray_popup"]["future_anchor_policy"], "screen-edge",
            "重置已知字段时必须保留同一对象中的未来字段"
        );
    }
}
