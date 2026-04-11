//! 旧格式 → 新格式的设置迁移逻辑
//!
//! 将旧版扁平格式的 AppSettings JSON 反序列化并转换为新版嵌套子结构体格式。

use super::{
    default_language, default_true, AppSettings, AppTheme, DisplaySettings, NotificationSettings,
    ProviderConfig, ProviderSettings, QuotaDisplayMode, SystemSettings, TrayIconStyle,
};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

/// 旧版扁平格式的 AppSettings，用于反序列化旧配置文件后迁移
#[derive(Debug, Deserialize)]
struct LegacyAppSettings {
    #[serde(default = "default_legacy_theme")]
    theme: AppTheme,
    #[serde(default = "default_legacy_refresh")]
    refresh_interval_mins: u64,
    #[serde(default = "default_legacy_hotkey")]
    global_hotkey: String,
    #[serde(default = "default_true")]
    auto_hide_window: bool,
    #[serde(default)]
    start_at_login: bool,
    #[serde(default = "default_true")]
    session_quota_notifications: bool,
    #[serde(default = "default_true")]
    notification_sound: bool,
    #[serde(default)]
    providers: ProviderSettings,
    #[serde(default)]
    enabled_providers: HashMap<String, bool>,
    #[serde(default)]
    provider_order: Vec<String>,
    #[serde(default = "default_language")]
    language: String,
    #[serde(default = "default_true")]
    show_dashboard_button: bool,
    #[serde(default = "default_true")]
    show_refresh_button: bool,
    #[serde(default)]
    show_debug_tab: bool,
    #[serde(default = "default_true")]
    show_account_info: bool,
    #[serde(default)]
    tray_icon_style: TrayIconStyle,
    #[serde(default)]
    quota_display_mode: QuotaDisplayMode,
    #[serde(default)]
    hidden_quotas: HashMap<String, HashSet<String>>,
}

/// 旧格式默认值 — 复用子结构体的默认值，保证一致性
fn default_legacy_theme() -> AppTheme {
    DisplaySettings::default().theme
}
fn default_legacy_refresh() -> u64 {
    SystemSettings::default().refresh_interval_mins
}
fn default_legacy_hotkey() -> String {
    SystemSettings::default().global_hotkey
}

impl From<LegacyAppSettings> for AppSettings {
    fn from(old: LegacyAppSettings) -> Self {
        Self {
            system: SystemSettings {
                auto_hide_window: old.auto_hide_window,
                start_at_login: old.start_at_login,
                refresh_interval_mins: old.refresh_interval_mins,
                global_hotkey: old.global_hotkey,
            },
            notification: NotificationSettings {
                session_quota_notifications: old.session_quota_notifications,
                notification_sound: old.notification_sound,
            },
            display: DisplaySettings {
                theme: old.theme,
                language: old.language,
                tray_icon_style: old.tray_icon_style,
                quota_display_mode: old.quota_display_mode,
                show_dashboard_button: old.show_dashboard_button,
                show_refresh_button: old.show_refresh_button,
                show_debug_tab: old.show_debug_tab,
                show_account_info: old.show_account_info,
                show_overview: true,
            },
            provider: ProviderConfig {
                credentials: old.providers,
                enabled_providers: old.enabled_providers,
                provider_order: old.provider_order,
                hidden_quotas: old.hidden_quotas,
                sidebar_providers: Vec::new(), // 由 ensure_sidebar_defaults() 填充
            },
        }
    }
}

impl AppSettings {
    /// 尝试从 JSON 值反序列化，支持新旧两种格式。
    /// 先尝试新格式（嵌套子结构体），失败后回退到旧格式（扁平字段）并自动迁移。
    pub fn from_json_value(value: serde_json::Value) -> Result<Self, serde_json::Error> {
        // 新格式：有 "system" / "display" / "notification" / "provider" 等顶层 key
        if value.get("system").is_some() || value.get("display").is_some() {
            return serde_json::from_value(value);
        }
        // 旧格式（或空 JSON {}）：扁平字段，迁移（空字段的默认值和 Default 一致）
        let legacy: LegacyAppSettings = serde_json::from_value(value)?;
        Ok(legacy.into())
    }
}
