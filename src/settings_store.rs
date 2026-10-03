use crate::models::{
    AppSettings, DisplaySettings, LoggingSettings, NotificationSettings, ProviderConfig,
    ProviderLayoutItem, ProviderSettings, QuotaRuleOverrides, QuotaRules, QuotaThresholdUnit,
    QuotaThresholds, SystemSettings,
};
use crate::platform::atomic_file::write_private_file_atomically;
use anyhow::{Context, Result};
use log::debug;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
struct PersistedQuotaThresholds {
    warning: f64,
    critical: f64,
    notify: f64,
}

fn parse_persisted_thresholds(
    raw: serde_json::Value,
    unit: QuotaThresholdUnit,
    scope: &str,
) -> Option<QuotaThresholds> {
    let thresholds = serde_json::from_value::<PersistedQuotaThresholds>(raw)
        .ok()
        .map(|p| QuotaThresholds {
            warning: p.warning,
            critical: p.critical,
            notify: p.notify,
        })
        .filter(|t| t.validate(unit).is_ok());
    if thresholds.is_none() {
        log::warn!(
            target: "settings",
            "invalid {scope} quota thresholds for {}; ignoring",
            unit.config_key()
        );
    }
    thresholds
}

/// 与阈值组同级的容错：非法 step 值只处理该条，不拖垮整个设置文件。
/// 非数字 → 丢弃该条（跟随全局）；数字越界 → clamp 到 0..=100（0 = 关闭该 provider 的提醒）。
fn parse_persisted_step_pct(key: &str, raw: serde_json::Value) -> Option<u8> {
    let step = raw.as_f64().map(|value| {
        if value.is_finite() && value > 0.0 {
            value.round().min(100.0) as u8
        } else {
            0
        }
    });
    if step.is_none() {
        log::warn!(target: "settings", "invalid quota usage step for {key}; ignoring");
    }
    step
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct PersistedQuotaRuleGroups {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    percentage: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    currency: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    amount: Option<serde_json::Value>,
}

impl PersistedQuotaRuleGroups {
    fn into_overrides(self, scope: &str) -> QuotaRuleOverrides {
        let mut overrides = QuotaRuleOverrides::default();
        for (unit, raw) in [
            (QuotaThresholdUnit::Percentage, self.percentage),
            (QuotaThresholdUnit::Currency, self.currency),
            (QuotaThresholdUnit::Amount, self.amount),
        ] {
            if let Some(thresholds) =
                raw.and_then(|value| parse_persisted_thresholds(value, unit, scope))
            {
                overrides.set(unit, Some(thresholds));
            }
        }
        overrides
    }

    fn into_rules(self) -> QuotaRules {
        QuotaRules::default().resolve(&self.into_overrides("global"))
    }

    fn from_overrides(overrides: &QuotaRuleOverrides) -> Self {
        let to_value = |t: Option<QuotaThresholds>| t.and_then(|t| serde_json::to_value(t).ok());
        Self {
            percentage: to_value(overrides.percentage),
            currency: to_value(overrides.currency),
            amount: to_value(overrides.amount),
        }
    }

    fn from_rules(rules: &QuotaRules) -> Self {
        Self::from_overrides(&QuotaRuleOverrides {
            percentage: Some(rules.percentage),
            currency: Some(rules.currency),
            amount: Some(rules.amount),
        })
    }
}

/// Provider 配置的持久化 DTO。
///
/// `provider_layout: None` 表示旧格式或首次启动；`Some([])` 表示用户明确配置为空。
/// 这一区分只存在于持久化边界，进入运行时领域模型前会完成迁移和归一化。
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct PersistedProviderConfig {
    credentials: ProviderSettings,
    hidden_quotas: HashMap<String, HashSet<String>>,
    // 值先保留为 Value 再逐条解析：手改 JSON 的非法条目只丢弃该条，不拖垮整个文件
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    quota_usage_steps: HashMap<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    quota_threshold_overrides: HashMap<String, PersistedQuotaRuleGroups>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider_layout: Option<Vec<ProviderLayoutItem>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    enabled_providers: Option<HashMap<String, bool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider_order: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sidebar_providers: Option<Vec<String>>,
}

impl PersistedProviderConfig {
    fn into_domain(self) -> ProviderConfig {
        let legacy_layout = migrate_legacy_provider_layout(&self);
        let layout = self.provider_layout.unwrap_or(legacy_layout);
        let quota_threshold_overrides = self
            .quota_threshold_overrides
            .into_iter()
            .filter_map(|(key, persisted)| {
                let overrides = persisted.into_overrides(&key);
                (!overrides.is_empty()).then_some((key, overrides))
            })
            .collect();
        let quota_usage_steps = self
            .quota_usage_steps
            .into_iter()
            .filter_map(|(key, raw)| parse_persisted_step_pct(&key, raw).map(|step| (key, step)))
            .collect();
        let mut config = ProviderConfig {
            credentials: self.credentials,
            provider_layout: layout,
            hidden_quotas: self.hidden_quotas,
            quota_usage_steps,
            quota_threshold_overrides,
        };
        config.normalize_layout();
        config
    }
}

fn migrate_legacy_provider_layout(value: &PersistedProviderConfig) -> Vec<ProviderLayoutItem> {
    let enabled = value.enabled_providers.as_ref();
    let order = value.provider_order.as_deref().unwrap_or_default();
    let sidebar: HashSet<&str> = value
        .sidebar_providers
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(String::as_str)
        .collect();

    let mut ids = Vec::new();
    let mut seen = HashSet::new();
    for key in order
        .iter()
        .chain(value.sidebar_providers.as_deref().unwrap_or_default())
        .chain(enabled.into_iter().flat_map(|map| map.keys()))
    {
        if seen.insert(key.clone()) {
            ids.push(key.clone());
        }
    }

    if ids.is_empty() {
        return ProviderConfig::default_layout();
    }

    for kind in crate::models::ProviderKind::all() {
        let key = kind.id_key().to_string();
        if seen.insert(key.clone()) {
            ids.push(key);
        }
    }

    ids.into_iter()
        .map(|id| {
            let is_enabled = enabled
                .and_then(|map| map.get(&id))
                .copied()
                .unwrap_or(false);
            // 旧 Overview/refresh 不要求 sidebar 成员资格。启用但未入栏的项必须
            // 带着 enabled 一起迁入 sidebar；若只按 sidebar 成员写 in_sidebar，
            // ProviderLayoutItem::new 会因不变量把 enabled 关掉，静默停止监控。
            ProviderLayoutItem::new(
                id.clone(),
                sidebar.contains(id.as_str()) || is_enabled,
                is_enabled,
            )
        })
        .collect()
}

/// settings.json 当前持久化版本。
///
/// 顶层 DTO 刻意与运行时 `AppSettings` 分离；Provider 旧字段的迁移也在此边界完成。
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct PersistedAppSettingsV1 {
    system: SystemSettings,
    notification: NotificationSettings,
    display: DisplaySettings,
    logging: LoggingSettings,
    provider: PersistedProviderConfig,
    quota: PersistedQuotaRuleGroups,
}

impl From<PersistedAppSettingsV1> for AppSettings {
    fn from(value: PersistedAppSettingsV1) -> Self {
        Self {
            system: value.system,
            notification: value.notification,
            display: value.display,
            logging: value.logging,
            provider: value.provider.into_domain(),
            quota: value.quota.into_rules(),
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
            provider: PersistedProviderConfig {
                credentials: value.provider.credentials.clone(),
                hidden_quotas: value.provider.hidden_quotas.clone(),
                quota_usage_steps: value
                    .provider
                    .quota_usage_steps
                    .iter()
                    .map(|(key, step)| (key.clone(), serde_json::Value::from(*step)))
                    .collect(),
                quota_threshold_overrides: value
                    .provider
                    .quota_threshold_overrides
                    .iter()
                    .filter(|(_, overrides)| !overrides.is_empty())
                    .map(|(key, overrides)| {
                        (
                            key.clone(),
                            PersistedQuotaRuleGroups::from_overrides(overrides),
                        )
                    })
                    .collect(),
                provider_layout: Some(value.provider.provider_layout.clone()),
                ..Default::default()
            },
            quota: PersistedQuotaRuleGroups::from_rules(&value.quota),
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

/// Provider 布局、凭证和 quota 可见性都是当前领域状态的完整快照。
///
/// 保存时整体替换这些字段，并删除已经迁移的旧字段，避免通用未知字段合并把旧状态
/// 再次带回，造成新旧 Provider 配置同时存在。
fn replace_dynamic_provider_maps(existing: &mut serde_json::Value, current: &serde_json::Value) {
    const LEGACY_FIELDS: [&str; 3] = ["enabled_providers", "provider_order", "sidebar_providers"];
    const CURRENT_FIELDS: [&str; 5] = [
        "credentials",
        "hidden_quotas",
        "provider_layout",
        "quota_usage_steps",
        "quota_threshold_overrides",
    ];

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

    for field in LEGACY_FIELDS {
        existing_provider.remove(field);
    }
    for field in CURRENT_FIELDS {
        match current_provider.get(field) {
            Some(current_value) => {
                existing_provider.insert(field.to_string(), current_value.clone());
            }
            None => {
                existing_provider.remove(field);
            }
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
    fn diag_load_builtin_enabled_records() {
        let (_dir, path) = temp_settings_path();
        fs::write(
            &path,
            r#"{
            "system": {"refresh_interval_mins": 5, "global_hotkey": "alt-1"},
            "notification": {"session_quota_notifications": true, "notification_sound": true},
            "display": {"theme": "Dark", "language": "system", "tray_icon_style": "Monochrome", "quota_display_mode": "Remaining", "show_dashboard_button": true, "show_refresh_button": true, "show_debug_tab": false, "show_account_info": true, "show_overview": true},
            "logging": {"max_bytes": 5242880, "max_files": 4},
            "provider": {
                "credentials": {},
                "enabled_providers": {"claude": true, "codex": true, "windsurf": true, "ciii:script": true, "suixiang:script": true},
                "provider_order": ["claude", "codex", "windsurf", "ciii:script", "suixiang:script"],
                "hidden_quotas": {},
                "sidebar_providers": ["claude", "codex", "windsurf", "ciii:script", "suixiang:script"]
            }
        }"#,
        )
        .unwrap();

        let loaded = load_from(&path).unwrap();
        let claude = crate::models::ProviderId::BuiltIn(crate::models::ProviderKind::Claude);
        let codex = crate::models::ProviderId::BuiltIn(crate::models::ProviderKind::Codex);
        let windsurf = crate::models::ProviderId::BuiltIn(crate::models::ProviderKind::Windsurf);
        assert!(
            loaded.provider.is_enabled(&claude),
            "claude enabled record lost on load!"
        );
        assert!(
            loaded.provider.is_enabled(&codex),
            "codex enabled record lost on load!"
        );
        assert!(
            loaded.provider.is_enabled(&windsurf),
            "windsurf enabled record lost on load!"
        );
        assert_eq!(loaded.system.global_hotkey, "alt-1");
    }

    #[test]
    fn legacy_empty_provider_state_gets_default_layout() {
        let (_dir, path) = temp_settings_path();
        fs::write(
            &path,
            r#"{
                "provider": {
                    "enabled_providers": {},
                    "provider_order": [],
                    "sidebar_providers": []
                }
            }"#,
        )
        .unwrap();

        let loaded = load_from(&path).unwrap();

        assert_eq!(
            loaded.provider.sidebar_provider_ids(&[]),
            vec![
                crate::models::ProviderId::BuiltIn(crate::models::ProviderKind::Claude),
                crate::models::ProviderId::BuiltIn(crate::models::ProviderKind::Codex),
            ]
        );
    }

    #[test]
    fn explicit_empty_provider_layout_is_preserved() {
        let (_dir, path) = temp_settings_path();
        fs::write(&path, r#"{"provider":{"provider_layout":[]}}"#).unwrap();

        let loaded = load_from(&path).unwrap();

        assert!(loaded.provider.provider_layout.is_empty());
        assert!(loaded.provider.sidebar_provider_ids(&[]).is_empty());
    }

    #[test]
    fn legacy_provider_layout_migration_preserves_custom_order_and_flags() {
        let (_dir, path) = temp_settings_path();
        fs::write(
            &path,
            r#"{
                "provider": {
                    "enabled_providers": {"gemini": true, "relay:api": false},
                    "provider_order": ["relay:api", "gemini"],
                    "sidebar_providers": ["gemini"]
                }
            }"#,
        )
        .unwrap();

        let loaded = load_from(&path).unwrap();
        let layout = &loaded.provider.provider_layout;

        assert_eq!(layout[0].id(), "relay:api");
        assert!(!layout[0].is_in_sidebar());
        assert!(!layout[0].is_enabled());
        assert_eq!(layout[1].id(), "gemini");
        assert!(layout[1].is_in_sidebar());
        assert!(layout[1].is_enabled());
    }

    #[test]
    fn legacy_enabled_provider_missing_from_sidebar_stays_enabled_and_joins_sidebar() {
        let (_dir, path) = temp_settings_path();
        fs::write(
            &path,
            r#"{
                "provider": {
                    "enabled_providers": {"windsurf": true, "claude": false},
                    "provider_order": ["claude", "windsurf"],
                    "sidebar_providers": ["claude"]
                }
            }"#,
        )
        .unwrap();

        let loaded = load_from(&path).unwrap();
        let layout = &loaded.provider.provider_layout;
        let claude = layout.iter().find(|item| item.id() == "claude").unwrap();
        let windsurf = layout.iter().find(|item| item.id() == "windsurf").unwrap();

        assert!(claude.is_in_sidebar());
        assert!(!claude.is_enabled());
        assert!(
            windsurf.is_in_sidebar(),
            "enabled-but-not-in-sidebar must be added to the sidebar"
        );
        assert!(
            windsurf.is_enabled(),
            "enabled-but-not-in-sidebar must stay enabled"
        );
    }

    #[test]
    fn new_provider_layout_normalizes_duplicate_and_invalid_items() {
        let (_dir, path) = temp_settings_path();
        fs::write(
            &path,
            r#"{
                "provider": {
                    "provider_layout": [
                        {"id": "gemini", "in_sidebar": false, "enabled": true},
                        {"id": "gemini", "in_sidebar": true, "enabled": true},
                        {"id": "", "in_sidebar": true, "enabled": true}
                    ]
                }
            }"#,
        )
        .unwrap();

        let loaded = load_from(&path).unwrap();

        assert_eq!(loaded.provider.provider_layout.len(), 1);
        assert_eq!(loaded.provider.provider_layout[0].id(), "gemini");
        assert!(!loaded.provider.provider_layout[0].is_in_sidebar());
        assert!(!loaded.provider.provider_layout[0].is_enabled());
    }

    #[test]
    fn saving_legacy_provider_config_removes_legacy_fields() {
        let (_dir, path) = temp_settings_path();
        fs::write(
            &path,
            r#"{
                "provider": {
                    "enabled_providers": {"claude": true},
                    "provider_order": ["claude"],
                    "sidebar_providers": ["claude"]
                }
            }"#,
        )
        .unwrap();

        let loaded = load_from(&path).unwrap();
        save_to(&loaded, &path).unwrap();

        let saved: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert!(saved["provider"].get("provider_layout").is_some());
        assert!(saved["provider"].get("enabled_providers").is_none());
        assert!(saved["provider"].get("provider_order").is_none());
        assert!(saved["provider"].get("sidebar_providers").is_none());
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
    fn save_persists_provider_layout_item_removal() {
        let (_dir, path) = temp_settings_path();
        let mut settings = AppSettings::default();
        let provider_id = crate::models::ProviderId::Custom("removed:newapi".to_string());
        settings.provider.set_enabled(&provider_id, true);
        save_to(&settings, &path).unwrap();

        settings.provider.remove_provider_references(&provider_id);
        save_to(&settings, &path).unwrap();

        let saved: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert!(!saved["provider"]["provider_layout"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["id"] == provider_id.id_key()));
        assert!(saved["provider"].get("enabled_providers").is_none());
        assert!(saved["provider"].get("provider_order").is_none());
        assert!(saved["provider"].get("sidebar_providers").is_none());
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

    #[test]
    fn legacy_json_defaults_quota_usage_step_to_zero_and_empty_map() {
        let (_dir, path) = temp_settings_path();
        fs::write(
            &path,
            r#"{
                "notification": {"session_quota_notifications": true, "notification_sound": true},
                "provider": {"credentials": {}, "hidden_quotas": {}}
            }"#,
        )
        .unwrap();

        let loaded = load_from(&path).unwrap();

        assert_eq!(loaded.notification.quota_usage_step_pct, 0);
        assert!(loaded.provider.quota_usage_steps.is_empty());
    }

    #[test]
    fn quota_usage_settings_round_trip() {
        use crate::models::{ProviderId, ProviderKind};

        let (_dir, path) = temp_settings_path();
        let mut settings = AppSettings::default();
        settings.notification.quota_usage_step_pct = 10;
        settings
            .provider
            .set_quota_usage_step(&ProviderId::BuiltIn(ProviderKind::Claude), Some(0));
        settings
            .provider
            .set_quota_usage_step(&ProviderId::Custom("myai:cli".to_string()), Some(5));
        save_to(&settings, &path).unwrap();

        let loaded = load_from(&path).unwrap();

        assert_eq!(loaded.notification.quota_usage_step_pct, 10);
        assert_eq!(
            loaded
                .provider
                .quota_usage_step(&ProviderId::BuiltIn(ProviderKind::Claude)),
            Some(0),
            "显式 0 覆盖必须在 round-trip 后保留（与跟随全局区分）"
        );
        assert_eq!(
            loaded
                .provider
                .quota_usage_step(&ProviderId::Custom("myai:cli".to_string())),
            Some(5)
        );
        assert_eq!(
            loaded
                .provider
                .effective_quota_usage_step(&ProviderId::BuiltIn(ProviderKind::Gemini), 10),
            10
        );
    }

    /// 手改 settings.json 写入非法 step 值：逐字段降级（clamp / 丢弃），
    /// 整个设置文件与其它字段不受影响。
    #[test]
    fn invalid_quota_usage_step_values_degrade_per_field() {
        use crate::models::{AppTheme, ProviderId, ProviderKind};

        let (_dir, path) = temp_settings_path();
        fs::write(
            &path,
            r#"{
                "notification": {"quota_usage_step_pct": 300},
                "display": {"theme": "Dark"},
                "provider": {
                    "credentials": {},
                    "hidden_quotas": {},
                    "quota_usage_steps": {"codex": 300, "kiro": -5, "amp": "abc"}
                }
            }"#,
        )
        .unwrap();

        let loaded = load_from(&path).expect("非法 step 不应让整个设置文件加载失败");
        assert_eq!(
            loaded.notification.quota_usage_step_pct, 100,
            "全局越界值应 clamp 到 100"
        );
        assert_eq!(
            loaded
                .provider
                .quota_usage_step(&ProviderId::BuiltIn(ProviderKind::Codex)),
            Some(100),
            "Provider 越界值应 clamp 到 100"
        );
        assert_eq!(
            loaded
                .provider
                .quota_usage_step(&ProviderId::BuiltIn(ProviderKind::Kiro)),
            Some(0),
            "负数按 0（关闭）处理"
        );
        assert_eq!(
            loaded
                .provider
                .quota_usage_step(&ProviderId::BuiltIn(ProviderKind::Amp)),
            None,
            "非数字条目应被丢弃（跟随全局）"
        );
        assert_eq!(loaded.display.theme, AppTheme::Dark, "其它设置字段必须保留");
    }

    #[test]
    fn negative_global_quota_usage_step_treated_as_disabled() {
        let (_dir, path) = temp_settings_path();
        fs::write(&path, r#"{"notification": {"quota_usage_step_pct": -1}}"#).unwrap();

        let loaded = load_from(&path).unwrap();
        assert_eq!(loaded.notification.quota_usage_step_pct, 0);
    }

    #[test]
    fn save_removes_cleared_quota_usage_step_override() {
        use crate::models::{ProviderId, ProviderKind};

        let (_dir, path) = temp_settings_path();
        let mut settings = AppSettings::default();
        settings
            .provider
            .set_quota_usage_step(&ProviderId::BuiltIn(ProviderKind::Claude), Some(5));
        save_to(&settings, &path).unwrap();

        settings
            .provider
            .set_quota_usage_step(&ProviderId::BuiltIn(ProviderKind::Claude), None);
        save_to(&settings, &path).unwrap();

        let saved: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert!(
            saved["provider"].get("quota_usage_steps").is_none()
                || saved["provider"]["quota_usage_steps"]
                    .as_object()
                    .is_some_and(|map| map.is_empty()),
            "清空后的 quota_usage_steps 不应残留旧覆盖"
        );

        let loaded = load_from(&path).unwrap();
        assert_eq!(
            loaded
                .provider
                .quota_usage_step(&ProviderId::BuiltIn(ProviderKind::Claude)),
            None
        );
    }

    #[test]
    fn quota_rules_round_trip() {
        use crate::models::{ProviderId, ProviderKind};

        let (_dir, path) = temp_settings_path();
        let custom_currency = QuotaThresholds {
            warning: 15.0,
            critical: 4.0,
            notify: 2.0,
        };
        let custom_percentage = QuotaThresholds {
            warning: 70.0,
            critical: 40.0,
            notify: 25.0,
        };
        let mut settings = AppSettings::default();
        settings.quota.currency = custom_currency;
        settings.provider.set_quota_threshold_override(
            &ProviderId::BuiltIn(ProviderKind::Claude),
            QuotaThresholdUnit::Percentage,
            Some(custom_percentage),
        );
        settings.provider.set_quota_threshold_override(
            &ProviderId::Custom("myai:cli".to_string()),
            QuotaThresholdUnit::Amount,
            Some(QuotaThresholds {
                warning: 200.0,
                critical: 50.0,
                notify: 25.0,
            }),
        );

        save_to(&settings, &path).unwrap();
        let loaded = load_from(&path).unwrap();

        assert_eq!(loaded.quota.currency, custom_currency);
        assert_eq!(loaded.quota.percentage, QuotaThresholds::DEFAULT_PERCENTAGE);
        assert_eq!(
            loaded.provider.quota_threshold_override(
                &ProviderId::BuiltIn(ProviderKind::Claude),
                QuotaThresholdUnit::Percentage
            ),
            Some(custom_percentage)
        );
        assert_eq!(
            loaded.provider.quota_threshold_override(
                &ProviderId::BuiltIn(ProviderKind::Claude),
                QuotaThresholdUnit::Amount
            ),
            None
        );
        assert!(loaded
            .provider
            .quota_threshold_override(
                &ProviderId::Custom("myai:cli".to_string()),
                QuotaThresholdUnit::Amount
            )
            .is_some());
    }

    #[test]
    fn missing_quota_section_uses_per_unit_defaults() {
        let (_dir, path) = temp_settings_path();
        fs::write(&path, r#"{"provider":{"provider_layout":[]}}"#).unwrap();

        let loaded = load_from(&path).unwrap();

        assert_eq!(loaded.quota, QuotaRules::default());
    }

    #[test]
    fn invalid_quota_group_falls_back_only_that_unit() {
        let (_dir, path) = temp_settings_path();
        fs::write(
            &path,
            r#"{
                "system": {"refresh_interval_mins": 42},
                "quota": {
                    "percentage": {"warning": 50, "critical": 40, "notify": 60},
                    "currency": {"warning": 15, "critical": 4, "notify": 2}
                }
            }"#,
        )
        .unwrap();

        let loaded = load_from(&path).unwrap();

        assert_eq!(loaded.quota.percentage, QuotaThresholds::DEFAULT_PERCENTAGE);
        assert_eq!(
            loaded.quota.currency,
            QuotaThresholds {
                warning: 15.0,
                critical: 4.0,
                notify: 2.0,
            }
        );
        assert_eq!(loaded.quota.amount, QuotaThresholds::DEFAULT_AMOUNT);
        assert_eq!(loaded.system.refresh_interval_mins, 42);
    }

    #[test]
    fn invalid_override_unit_falls_back_to_global() {
        use crate::models::{ProviderId, ProviderKind};

        let (_dir, path) = temp_settings_path();
        fs::write(
            &path,
            r#"{
                "provider": {
                    "quota_threshold_overrides": {
                        "claude": {
                            "percentage": {"warning": 70, "critical": 40, "notify": 25},
                            "currency": {"warning": "high", "critical": 4, "notify": 2}
                        }
                    }
                }
            }"#,
        )
        .unwrap();

        let loaded = load_from(&path).unwrap();
        let claude = ProviderId::BuiltIn(ProviderKind::Claude);

        assert_eq!(
            loaded
                .provider
                .quota_threshold_override(&claude, QuotaThresholdUnit::Currency),
            None,
            "非法覆盖应被视为未覆盖（继承全局）"
        );
        assert_eq!(
            loaded
                .provider
                .quota_threshold_override(&claude, QuotaThresholdUnit::Percentage)
                .map(|t| t.warning),
            Some(70.0)
        );
    }

    #[test]
    fn save_removes_cleared_quota_threshold_override() {
        use crate::models::{ProviderId, ProviderKind};

        let (_dir, path) = temp_settings_path();
        let claude = ProviderId::BuiltIn(ProviderKind::Claude);
        let mut settings = AppSettings::default();
        settings.provider.set_quota_threshold_override(
            &claude,
            QuotaThresholdUnit::Percentage,
            Some(QuotaThresholds {
                warning: 70.0,
                critical: 40.0,
                notify: 25.0,
            }),
        );
        save_to(&settings, &path).unwrap();

        settings.provider.set_quota_threshold_override(
            &claude,
            QuotaThresholdUnit::Percentage,
            None,
        );
        save_to(&settings, &path).unwrap();

        let saved: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert!(
            saved["provider"].get("quota_threshold_overrides").is_none()
                || saved["provider"]["quota_threshold_overrides"]
                    .as_object()
                    .is_none_or(|map| !map.contains_key("claude")),
            "清空后的 quota_threshold_overrides 不应残留 claude"
        );

        let loaded = load_from(&path).unwrap();
        assert_eq!(
            loaded
                .provider
                .quota_threshold_override(&claude, QuotaThresholdUnit::Percentage),
            None
        );
    }

    #[test]
    fn cleared_unit_in_multi_unit_override_does_not_resurrect() {
        use crate::models::{ProviderId, ProviderKind};

        let (_dir, path) = temp_settings_path();
        let claude = ProviderId::BuiltIn(ProviderKind::Claude);
        let percentage = QuotaThresholds {
            warning: 70.0,
            critical: 40.0,
            notify: 25.0,
        };
        let currency = QuotaThresholds {
            warning: 20.0,
            critical: 5.0,
            notify: 2.0,
        };
        let mut settings = AppSettings::default();
        settings.provider.set_quota_threshold_override(
            &claude,
            QuotaThresholdUnit::Percentage,
            Some(percentage),
        );
        settings.provider.set_quota_threshold_override(
            &claude,
            QuotaThresholdUnit::Currency,
            Some(currency),
        );
        save_to(&settings, &path).unwrap();

        settings.provider.set_quota_threshold_override(
            &claude,
            QuotaThresholdUnit::Percentage,
            None,
        );
        save_to(&settings, &path).unwrap();

        let loaded = load_from(&path).unwrap();
        assert_eq!(
            loaded
                .provider
                .quota_threshold_override(&claude, QuotaThresholdUnit::Percentage),
            None,
            "已清除的单位不得复活"
        );
        assert_eq!(
            loaded
                .provider
                .quota_threshold_override(&claude, QuotaThresholdUnit::Currency),
            Some(currency)
        );
    }

    #[test]
    fn global_and_provider_quota_groups_share_raw_shape() {
        use crate::models::{ProviderId, ProviderKind};

        let (_dir, path) = temp_settings_path();
        let claude = ProviderId::BuiltIn(ProviderKind::Claude);
        let thresholds = QuotaThresholds {
            warning: 70.0,
            critical: 40.0,
            notify: 25.0,
        };
        let mut settings = AppSettings::default();
        settings.quota.percentage = thresholds;
        settings.provider.set_quota_threshold_override(
            &claude,
            QuotaThresholdUnit::Percentage,
            Some(thresholds),
        );
        save_to(&settings, &path).unwrap();

        let saved: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let global_group = &saved["quota"]["percentage"];
        let provider_group =
            &saved["provider"]["quota_threshold_overrides"]["claude"]["percentage"];
        for group in [global_group, provider_group] {
            let obj = group
                .as_object()
                .expect("threshold group must be a flat object");
            let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
            keys.sort_unstable();
            assert_eq!(
                keys,
                ["critical", "notify", "warning"],
                "group must be raw warning/critical/notify without tag/type wrapper: {group}"
            );
        }
        assert_eq!(global_group, provider_group);
    }

    #[test]
    fn partial_global_quota_fills_other_unit_defaults() {
        let (_dir, path) = temp_settings_path();
        fs::write(
            &path,
            r#"{
                "quota": {
                    "percentage": {"warning": 70, "critical": 40, "notify": 25}
                }
            }"#,
        )
        .unwrap();

        let loaded = load_from(&path).unwrap();

        assert_eq!(
            loaded.quota.percentage,
            QuotaThresholds {
                warning: 70.0,
                critical: 40.0,
                notify: 25.0,
            }
        );
        assert_eq!(loaded.quota.currency, QuotaThresholds::DEFAULT_CURRENCY);
        assert_eq!(loaded.quota.amount, QuotaThresholds::DEFAULT_AMOUNT);
    }

    #[test]
    fn invalid_provider_override_unit_inherits_non_default_global() {
        use crate::models::{ProviderId, ProviderKind};

        let (_dir, path) = temp_settings_path();
        let claude = ProviderId::BuiltIn(ProviderKind::Claude);
        fs::write(
            &path,
            r#"{
                "quota": {
                    "currency": {"warning": 15, "critical": 4, "notify": 2}
                },
                "provider": {
                    "quota_threshold_overrides": {
                        "claude": {
                            "currency": {"warning": "high", "critical": 4, "notify": 2}
                        }
                    }
                }
            }"#,
        )
        .unwrap();

        let loaded = load_from(&path).unwrap();

        assert_eq!(
            loaded
                .provider
                .quota_threshold_override(&claude, QuotaThresholdUnit::Currency),
            None
        );
        assert_eq!(
            loaded.effective_quota_rules(&claude).currency,
            QuotaThresholds {
                warning: 15.0,
                critical: 4.0,
                notify: 2.0,
            },
            "非法覆盖必须继承当前全局规则而非库默认"
        );
    }
}
