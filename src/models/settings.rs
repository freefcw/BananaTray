use super::provider::{ProviderId, ProviderKind};
use super::quota::QuotaInfo;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

// ============================================================================
// 子结构体定义（按语义职责分组）
// ============================================================================

/// 系统行为设置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemSettings {
    pub auto_hide_window: bool,
    /// 开机自启动
    #[serde(default)]
    pub start_at_login: bool,
    /// 自动刷新间隔（分钟），0 表示禁用自动刷新
    pub refresh_interval_mins: u64,
    pub global_hotkey: String,
}

impl Default for SystemSettings {
    fn default() -> Self {
        Self {
            auto_hide_window: true,
            start_at_login: false,
            refresh_interval_mins: 5,
            global_hotkey: "Cmd+Shift+S".to_string(),
        }
    }
}

/// 通知设置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationSettings {
    /// Session 配额变更通知
    #[serde(default = "default_true")]
    pub session_quota_notifications: bool,
    /// 通知是否带声音
    #[serde(default = "default_true")]
    pub notification_sound: bool,
}

impl Default for NotificationSettings {
    fn default() -> Self {
        Self {
            session_quota_notifications: true,
            notification_sound: true,
        }
    }
}

/// 显示/外观设置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplaySettings {
    pub theme: AppTheme,
    /// 界面语言（"system" 表示跟随系统，"en" / "zh-CN" 等为具体语言）
    #[serde(default = "default_language")]
    pub language: String,
    /// 托盘图标风格
    #[serde(default)]
    pub tray_icon_style: TrayIconStyle,
    /// 额度显示模式：剩余 or 已用
    #[serde(default)]
    pub quota_display_mode: QuotaDisplayMode,
    /// 是否在工具栏显示 Dashboard 按钮
    #[serde(default = "default_true")]
    pub show_dashboard_button: bool,
    /// 是否在工具栏显示 Refresh 按钮
    #[serde(default = "default_true")]
    pub show_refresh_button: bool,
    /// 是否显示 Debug 标签页
    #[serde(default)]
    pub show_debug_tab: bool,
    /// 是否在 Provider 面板显示账户信息卡片
    #[serde(default = "default_true")]
    pub show_account_info: bool,
}

impl Default for DisplaySettings {
    fn default() -> Self {
        Self {
            theme: AppTheme::Dark,
            language: default_language(),
            tray_icon_style: TrayIconStyle::default(),
            quota_display_mode: QuotaDisplayMode::default(),
            show_dashboard_button: true,
            show_refresh_button: true,
            show_debug_tab: false,
            show_account_info: true,
        }
    }
}

/// Provider 管理配置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderConfig {
    /// Provider 特定配置（Copilot Token 等）
    pub credentials: ProviderSettings,
    /// 各 Provider 启用状态（key = provider id_key, value = enabled）
    #[serde(default)]
    pub enabled_providers: HashMap<String, bool>,
    /// Provider 在导航栏中的排列顺序（存储 id_key 列表）
    #[serde(default)]
    pub provider_order: Vec<String>,
    /// 每个 Provider 中被隐藏的配额标签集合（不在托盘弹窗中显示）
    /// key = provider id_key (如 "claude"), value = 隐藏的 quota label 集合
    #[serde(default)]
    pub hidden_quotas: HashMap<String, HashSet<String>>,
}

// ── ProviderConfig 方法（逻辑归属于此，AppSettings 薄委托） ──
impl ProviderConfig {
    /// 检查指定 Provider 是否已启用
    pub fn is_enabled(&self, id: &ProviderId) -> bool {
        self.enabled_providers
            .get(&id.id_key())
            .copied()
            .unwrap_or(false)
    }

    /// 设置指定 Provider 的启用状态（按 ProviderKind）
    pub fn set_provider_enabled(&mut self, kind: ProviderKind, enabled: bool) {
        self.enabled_providers
            .insert(kind.id_key().to_string(), enabled);
    }

    /// 通过 ProviderId 设置启用状态
    pub fn set_enabled(&mut self, id: &ProviderId, enabled: bool) {
        self.enabled_providers
            .insert(id.id_key().to_string(), enabled);
    }

    /// 按用户自定义顺序返回所有内置 Provider。未在 provider_order 中出现的追加到末尾。
    pub fn ordered_providers(&self) -> Vec<ProviderKind> {
        let mut result = Vec::with_capacity(ProviderKind::all().len());
        let mut seen = HashSet::with_capacity(ProviderKind::all().len());

        for key in &self.provider_order {
            if let Some(kind) = ProviderKind::from_id_key(key) {
                if seen.insert(kind) {
                    result.push(kind);
                }
            }
        }

        for &kind in ProviderKind::all() {
            if seen.insert(kind) {
                result.push(kind);
            }
        }

        result
    }

    /// 按用户自定义顺序返回所有 Provider（内置 + 自定义）。
    pub fn ordered_provider_ids(&self, custom_ids: &[ProviderId]) -> Vec<ProviderId> {
        let mut result = Vec::new();
        let mut seen = HashSet::new();

        for key in &self.provider_order {
            let id = ProviderId::from_id_key(key);
            if seen.insert(id.clone()) {
                result.push(id);
            }
        }

        for &kind in ProviderKind::all() {
            let id = ProviderId::BuiltIn(kind);
            if seen.insert(id.clone()) {
                result.push(id);
            }
        }

        for custom_id in custom_ids {
            if seen.insert(custom_id.clone()) {
                result.push(custom_id.clone());
            }
        }

        result
    }

    /// 将指定 Provider 在排序中上移一位。返回 true 表示发生了移动。
    pub fn move_provider_up(&mut self, id: &ProviderId, custom_ids: &[ProviderId]) -> bool {
        self.ensure_order(custom_ids);
        let key = id.id_key();
        if let Some(pos) = self.provider_order.iter().position(|k| *k == key) {
            if pos > 0 {
                self.provider_order.swap(pos, pos - 1);
                return true;
            }
        }
        false
    }

    /// 将指定 Provider 在排序中下移一位。返回 true 表示发生了移动。
    pub fn move_provider_down(&mut self, id: &ProviderId, custom_ids: &[ProviderId]) -> bool {
        self.ensure_order(custom_ids);
        let key = id.id_key();
        if let Some(pos) = self.provider_order.iter().position(|k| *k == key) {
            if pos + 1 < self.provider_order.len() {
                self.provider_order.swap(pos, pos + 1);
                return true;
            }
        }
        false
    }

    /// 判断某个 quota 是否在托盘弹窗中可见（未被隐藏）
    /// `quota_key` 应使用 `QuotaType::stable_key()`，而非 i18n label
    pub fn is_quota_visible(&self, kind: ProviderKind, quota_key: &str) -> bool {
        self.hidden_quotas
            .get(kind.id_key())
            .is_none_or(|set| !set.contains(quota_key))
    }

    /// 统计可见配额数量
    pub fn visible_quota_count(&self, kind: ProviderKind, quotas: &[QuotaInfo]) -> usize {
        quotas
            .iter()
            .filter(|q| self.is_quota_visible(kind, &q.quota_type.stable_key()))
            .count()
    }

    /// 过滤出在托盘弹窗中可见的配额
    pub fn visible_quotas<'a>(
        &self,
        kind: ProviderKind,
        quotas: &'a [QuotaInfo],
    ) -> Vec<&'a QuotaInfo> {
        quotas
            .iter()
            .filter(|q| self.is_quota_visible(kind, &q.quota_type.stable_key()))
            .collect()
    }

    /// 切换某个 quota 的可见性（隐藏 ↔ 显示）
    pub fn toggle_quota_visibility(&mut self, kind: ProviderKind, quota_key: String) {
        let set = self
            .hidden_quotas
            .entry(kind.id_key().to_string())
            .or_default();
        if !set.remove(&quota_key) {
            set.insert(quota_key);
        }
    }

    /// 确保 provider_order 包含所有 Provider（内置 + 自定义）
    fn ensure_order(&mut self, custom_ids: &[ProviderId]) {
        self.provider_order = self
            .ordered_provider_ids(custom_ids)
            .into_iter()
            .map(|id| id.id_key().to_string())
            .collect();
    }
}

// ============================================================================
// 枚举类型
// ============================================================================

/// 托盘图标风格
///
/// macOS 的 NSImage `setTemplate:YES` 会强制将图标当作模板图像，
/// 只读取 alpha 通道并忽略所有颜色信息，由系统根据菜单栏明暗模式
/// 自动着色（浅色模式 → 深色图标，深色模式 → 浅色图标）。
///
/// 为了支持彩色图标，`Monochrome` 使用 template 模式（跟随系统），
/// 而 `Yellow` / `Colorful` 则通过运行时 hack 将 `setTemplate` 关闭，
/// 使图标显示原始颜色。
///
/// 在 Windows / Linux 上没有 template 概念，PNG 颜色直接生效。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrayIconStyle {
    /// 单色 — macOS template 模式，跟随系统深色/浅色自动适配
    #[default]
    Monochrome,
    /// 黄色香蕉
    Yellow,
    /// 多彩渐变色香蕉
    Colorful,
}

/// 额度显示模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum QuotaDisplayMode {
    /// 显示剩余额度（默认）
    #[default]
    Remaining,
    /// 显示已用额度
    Used,
}

/// 应用主题
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppTheme {
    Light,
    Dark,
    System,
}

impl AppTheme {
    /// 将 System 解析为具体的 Light 或 Dark
    ///
    /// `system_is_dark` 由调用方提供（从平台 API 检测），
    /// 保持数据模型不依赖系统调用（DIP/可测试性）。
    pub fn resolve(self, system_is_dark: bool) -> AppTheme {
        match self {
            AppTheme::System => {
                if system_is_dark {
                    AppTheme::Dark
                } else {
                    AppTheme::Light
                }
            }
            other => other,
        }
    }
}

/// Provider 特定配置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderSettings {
    /// Copilot: GitHub Token (Classic PAT with copilot scope)
    pub github_token: Option<String>,
}

// ============================================================================
// 应用设置（顶层）
// ============================================================================

/// 应用配置 — 按职责分为四组子设置
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppSettings {
    /// 系统行为：自动隐藏、开机自启、刷新间隔、全局热键
    pub system: SystemSettings,
    /// 通知：配额通知、通知声音
    pub notification: NotificationSettings,
    /// 显示/外观：主题、语言、托盘图标、各 UI 开关
    pub display: DisplaySettings,
    /// Provider 管理：启用状态、排序、隐藏配额、Copilot Token
    pub provider: ProviderConfig,
}

fn default_true() -> bool {
    true
}

fn default_language() -> String {
    "system".to_string()
}

// ── AppSettings 薄委托方法 ──
// 高频调用路径的便捷方法，委托给 ProviderConfig，减少调用方链式层级。
impl AppSettings {
    pub fn is_enabled(&self, id: &ProviderId) -> bool {
        self.provider.is_enabled(id)
    }

    pub fn set_provider_enabled(&mut self, kind: ProviderKind, enabled: bool) {
        self.provider.set_provider_enabled(kind, enabled);
    }

    pub fn set_enabled(&mut self, id: &ProviderId, enabled: bool) {
        self.provider.set_enabled(id, enabled);
    }

    pub fn ordered_providers(&self) -> Vec<ProviderKind> {
        self.provider.ordered_providers()
    }

    pub fn ordered_provider_ids(&self, custom_ids: &[ProviderId]) -> Vec<ProviderId> {
        self.provider.ordered_provider_ids(custom_ids)
    }

    pub fn move_provider_up(&mut self, id: &ProviderId, custom_ids: &[ProviderId]) -> bool {
        self.provider.move_provider_up(id, custom_ids)
    }

    pub fn move_provider_down(&mut self, id: &ProviderId, custom_ids: &[ProviderId]) -> bool {
        self.provider.move_provider_down(id, custom_ids)
    }

    pub fn is_quota_visible(&self, kind: ProviderKind, quota_key: &str) -> bool {
        self.provider.is_quota_visible(kind, quota_key)
    }

    pub fn visible_quota_count(&self, kind: ProviderKind, quotas: &[QuotaInfo]) -> usize {
        self.provider.visible_quota_count(kind, quotas)
    }

    pub fn visible_quotas<'a>(
        &self,
        kind: ProviderKind,
        quotas: &'a [QuotaInfo],
    ) -> Vec<&'a QuotaInfo> {
        self.provider.visible_quotas(kind, quotas)
    }

    pub fn toggle_quota_visibility(&mut self, kind: ProviderKind, quota_key: String) {
        self.provider.toggle_quota_visibility(kind, quota_key);
    }
}

// ============================================================================
// 旧格式迁移
// ============================================================================

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
            },
            provider: ProviderConfig {
                credentials: old.providers,
                enabled_providers: old.enabled_providers,
                provider_order: old.provider_order,
                hidden_quotas: old.hidden_quotas,
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

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ── ProviderConfig 核心逻辑测试 ──────────────────────

    #[test]
    fn provider_config_is_enabled_default_false() {
        let config = ProviderConfig::default();
        assert!(!config.is_enabled(&ProviderId::BuiltIn(ProviderKind::Claude)));
    }

    #[test]
    fn provider_config_set_and_check_enabled() {
        let mut config = ProviderConfig::default();
        config.set_provider_enabled(ProviderKind::Claude, true);
        assert!(config.is_enabled(&ProviderId::BuiltIn(ProviderKind::Claude)));
        assert!(!config.is_enabled(&ProviderId::BuiltIn(ProviderKind::Gemini)));
    }

    #[test]
    fn provider_config_ordered_providers_ignores_invalid() {
        let config = ProviderConfig {
            provider_order: vec![
                "gemini".into(),
                "invalid".into(),
                "claude".into(),
                "gemini".into(), // duplicate
            ],
            ..Default::default()
        };

        let ordered = config.ordered_providers();
        assert_eq!(ordered[0], ProviderKind::Gemini);
        assert_eq!(ordered[1], ProviderKind::Claude);
        assert_eq!(ordered.len(), ProviderKind::all().len());
    }

    #[test]
    fn provider_config_quota_visibility() {
        let mut config = ProviderConfig::default();
        assert!(config.is_quota_visible(ProviderKind::Claude, "session"));

        config.toggle_quota_visibility(ProviderKind::Claude, "session".to_string());
        assert!(!config.is_quota_visible(ProviderKind::Claude, "session"));
        // 其他 provider 不受影响
        assert!(config.is_quota_visible(ProviderKind::Gemini, "session"));

        config.toggle_quota_visibility(ProviderKind::Claude, "session".to_string());
        assert!(config.is_quota_visible(ProviderKind::Claude, "session"));
    }

    #[test]
    fn provider_config_move_up_normalizes_order() {
        let mut config = ProviderConfig {
            provider_order: vec!["gemini".into(), "gemini".into(), "claude".into()],
            ..Default::default()
        };

        let claude = ProviderId::BuiltIn(ProviderKind::Claude);
        assert!(config.move_provider_up(&claude, &[]));
        assert_eq!(config.provider_order[0], ProviderKind::Claude.id_key());
        assert_eq!(config.provider_order[1], ProviderKind::Gemini.id_key());
        assert_eq!(config.provider_order.len(), ProviderKind::all().len());
    }

    // ── AppSettings 薄委托测试（验证委托正确性）──────────

    #[test]
    fn app_settings_delegates_is_enabled() {
        let mut settings = AppSettings::default();
        settings.set_provider_enabled(ProviderKind::Claude, true);
        assert!(settings.is_enabled(&ProviderId::BuiltIn(ProviderKind::Claude)));
    }

    #[test]
    fn ordered_providers_ignores_invalid_and_duplicate_keys() {
        let settings = AppSettings {
            provider: ProviderConfig {
                provider_order: vec![
                    "gemini".into(),
                    "invalid".into(),
                    "claude".into(),
                    "gemini".into(),
                ],
                ..Default::default()
            },
            ..Default::default()
        };

        let ordered = settings.ordered_providers();
        assert_eq!(ordered[0], ProviderKind::Gemini);
        assert_eq!(ordered[1], ProviderKind::Claude);
        assert_eq!(ordered.len(), ProviderKind::all().len());
    }

    #[test]
    fn move_provider_up_normalizes_provider_order() {
        let mut settings = AppSettings {
            provider: ProviderConfig {
                provider_order: vec!["gemini".into(), "gemini".into(), "claude".into()],
                ..Default::default()
            },
            ..Default::default()
        };

        let claude = ProviderId::BuiltIn(ProviderKind::Claude);
        assert!(settings.move_provider_up(&claude, &[]));
        assert_eq!(
            settings.provider.provider_order[0],
            ProviderKind::Claude.id_key()
        );
        assert_eq!(
            settings.provider.provider_order[1],
            ProviderKind::Gemini.id_key()
        );
        assert_eq!(
            settings.provider.provider_order.len(),
            ProviderKind::all().len()
        );
    }

    // ── TrayIconStyle ────────────────────────────────────

    #[test]
    fn tray_icon_style_default_is_monochrome() {
        assert_eq!(TrayIconStyle::default(), TrayIconStyle::Monochrome);
    }

    #[test]
    fn tray_icon_style_serde_round_trip() {
        for style in [
            TrayIconStyle::Monochrome,
            TrayIconStyle::Yellow,
            TrayIconStyle::Colorful,
        ] {
            let json = serde_json::to_string(&style).unwrap();
            let deserialized: TrayIconStyle = serde_json::from_str(&json).unwrap();
            assert_eq!(style, deserialized);
        }
    }

    // ── 新/旧格式序列化 ──────────────────────────────────

    #[test]
    fn app_settings_new_format_round_trip() {
        let settings = AppSettings::default();
        let json = serde_json::to_value(&settings).unwrap();
        let restored: AppSettings = serde_json::from_value(json).unwrap();
        assert_eq!(restored.display.tray_icon_style, TrayIconStyle::Monochrome);
        assert_eq!(restored.system.refresh_interval_mins, 5);
    }

    #[test]
    fn app_settings_legacy_migration() {
        let json = serde_json::json!({
            "theme": "Dark",
            "refresh_interval_mins": 10,
            "global_hotkey": "Cmd+Shift+S",
            "auto_hide_window": true,
            "providers": {},
            "language": "zh-CN",
            "show_debug_tab": true,
            "tray_icon_style": "Yellow"
        });
        let settings = AppSettings::from_json_value(json).unwrap();
        assert_eq!(settings.display.theme, AppTheme::Dark);
        assert_eq!(settings.system.refresh_interval_mins, 10);
        assert_eq!(settings.display.language, "zh-CN");
        assert!(settings.display.show_debug_tab);
        assert_eq!(settings.display.tray_icon_style, TrayIconStyle::Yellow);
        // 默认值
        assert!(settings.notification.session_quota_notifications);
        assert!(settings.display.show_dashboard_button);
    }

    #[test]
    fn app_settings_empty_json_returns_defaults() {
        let json = serde_json::json!({});
        let settings = AppSettings::from_json_value(json).unwrap();
        assert_eq!(settings.display.theme, AppTheme::Dark);
        assert_eq!(settings.system.refresh_interval_mins, 5);
        assert!(settings.system.auto_hide_window);
    }

    // ── hidden_quotas ────────────────────────────────────

    #[test]
    fn hidden_quotas_default_all_visible() {
        let settings = AppSettings::default();
        assert!(settings.is_quota_visible(ProviderKind::Claude, "session"));
        assert!(settings.is_quota_visible(ProviderKind::Claude, "model:Opus"));
    }

    #[test]
    fn toggle_quota_visibility_hides_then_shows() {
        let mut settings = AppSettings::default();
        assert!(settings.is_quota_visible(ProviderKind::Claude, "model:Opus"));

        settings.toggle_quota_visibility(ProviderKind::Claude, "model:Opus".to_string());
        assert!(!settings.is_quota_visible(ProviderKind::Claude, "model:Opus"));
        assert!(settings.is_quota_visible(ProviderKind::Claude, "model:Sonnet"));

        settings.toggle_quota_visibility(ProviderKind::Claude, "model:Opus".to_string());
        assert!(settings.is_quota_visible(ProviderKind::Claude, "model:Opus"));
    }

    #[test]
    fn hidden_quotas_isolated_per_provider() {
        let mut settings = AppSettings::default();
        settings.toggle_quota_visibility(ProviderKind::Claude, "session".to_string());

        assert!(!settings.is_quota_visible(ProviderKind::Claude, "session"));
        assert!(settings.is_quota_visible(ProviderKind::Gemini, "session"));
    }

    // ── ordered_provider_ids ──────────────────────────────

    #[test]
    fn ordered_provider_ids_respects_saved_order() {
        let settings = AppSettings {
            provider: ProviderConfig {
                provider_order: vec!["gemini".into(), "claude".into()],
                ..Default::default()
            },
            ..Default::default()
        };

        let ids = settings.ordered_provider_ids(&[]);
        assert_eq!(ids[0], ProviderId::BuiltIn(ProviderKind::Gemini));
        assert_eq!(ids[1], ProviderId::BuiltIn(ProviderKind::Claude));
        assert!(ids.len() >= ProviderKind::all().len());
    }

    #[test]
    fn ordered_provider_ids_includes_custom() {
        let settings = AppSettings {
            provider: ProviderConfig {
                provider_order: vec!["gemini".into(), "myai:cli".into(), "claude".into()],
                ..Default::default()
            },
            ..Default::default()
        };
        let custom = vec![ProviderId::Custom("myai:cli".to_string())];

        let ids = settings.ordered_provider_ids(&custom);
        let pos_gemini = ids
            .iter()
            .position(|id| *id == ProviderId::BuiltIn(ProviderKind::Gemini))
            .unwrap();
        let pos_custom = ids
            .iter()
            .position(|id| *id == ProviderId::Custom("myai:cli".to_string()))
            .unwrap();
        let pos_claude = ids
            .iter()
            .position(|id| *id == ProviderId::BuiltIn(ProviderKind::Claude))
            .unwrap();
        assert!(pos_gemini < pos_custom);
        assert!(pos_custom < pos_claude);
    }

    #[test]
    fn ordered_provider_ids_appends_unseen_custom() {
        let settings = AppSettings::default();
        let custom = vec![ProviderId::Custom("new:provider".to_string())];

        let ids = settings.ordered_provider_ids(&custom);
        assert!(ids.contains(&ProviderId::Custom("new:provider".to_string())));
        assert_eq!(ids.len(), ProviderKind::all().len() + 1);
    }

    #[test]
    fn ordered_provider_ids_deduplicates() {
        let settings = AppSettings {
            provider: ProviderConfig {
                provider_order: vec!["claude".into(), "claude".into(), "gemini".into()],
                ..Default::default()
            },
            ..Default::default()
        };

        let ids = settings.ordered_provider_ids(&[]);
        let claude_count = ids
            .iter()
            .filter(|id| **id == ProviderId::BuiltIn(ProviderKind::Claude))
            .count();
        assert_eq!(claude_count, 1);
    }

    // ── move_provider_up/down ─────────────────────────────

    #[test]
    fn move_provider_down_works() {
        let mut settings = AppSettings {
            provider: ProviderConfig {
                provider_order: vec!["claude".into(), "gemini".into()],
                ..Default::default()
            },
            ..Default::default()
        };

        let claude = ProviderId::BuiltIn(ProviderKind::Claude);
        assert!(settings.move_provider_down(&claude, &[]));
        let pos_claude = settings
            .provider
            .provider_order
            .iter()
            .position(|k| k == "claude")
            .unwrap();
        let pos_gemini = settings
            .provider
            .provider_order
            .iter()
            .position(|k| k == "gemini")
            .unwrap();
        assert!(pos_gemini < pos_claude);
    }

    #[test]
    fn move_provider_up_at_top_returns_false() {
        let mut settings = AppSettings {
            provider: ProviderConfig {
                provider_order: vec!["claude".into(), "gemini".into()],
                ..Default::default()
            },
            ..Default::default()
        };

        let claude = ProviderId::BuiltIn(ProviderKind::Claude);
        assert!(!settings.move_provider_up(&claude, &[]));
    }

    #[test]
    fn move_provider_down_at_bottom_returns_false() {
        let all_keys: Vec<String> = ProviderKind::all()
            .iter()
            .map(|k| k.id_key().to_string())
            .collect();
        let mut settings = AppSettings {
            provider: ProviderConfig {
                provider_order: all_keys,
                ..Default::default()
            },
            ..Default::default()
        };

        let last = ProviderKind::all().last().unwrap();
        let id = ProviderId::BuiltIn(*last);
        assert!(!settings.move_provider_down(&id, &[]));
    }

    #[test]
    fn move_custom_provider_up() {
        let custom = ProviderId::Custom("myai:cli".to_string());
        let mut settings = AppSettings {
            provider: ProviderConfig {
                provider_order: vec!["claude".into(), "myai:cli".into()],
                ..Default::default()
            },
            ..Default::default()
        };

        assert!(settings.move_provider_up(&custom, std::slice::from_ref(&custom)));
        assert_eq!(settings.provider.provider_order[0], "myai:cli");
        assert_eq!(settings.provider.provider_order[1], "claude");
    }
}
