use super::provider::{ProviderId, ProviderKind};
use super::quota::QuotaInfo;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

// ============================================================================
// 应用设置
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

/// 应用配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub theme: AppTheme,
    /// 自动刷新间隔（分钟），0 表示禁用自动刷新
    pub refresh_interval_mins: u64,
    pub global_hotkey: String,
    pub auto_hide_window: bool,
    /// 开机自启动
    #[serde(default)]
    pub start_at_login: bool,
    /// Session 配额变更通知
    #[serde(default = "default_true")]
    pub session_quota_notifications: bool,
    /// 通知是否带声音
    #[serde(default = "default_true")]
    pub notification_sound: bool,
    /// Provider 特定配置
    pub providers: ProviderSettings,
    /// 各 Provider 启用状态（key = provider id_key, value = enabled）
    #[serde(default)]
    pub enabled_providers: HashMap<String, bool>,
    /// Provider 在导航栏中的排列顺序（存储 id_key 列表）
    #[serde(default)]
    pub provider_order: Vec<String>,
    /// 界面语言（"system" 表示跟随系统，"en" / "zh-CN" 等为具体语言）
    #[serde(default = "default_language")]
    pub language: String,
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
    /// 托盘图标风格
    #[serde(default)]
    pub tray_icon_style: TrayIconStyle,
    /// 额度显示模式：剩余 or 已用
    #[serde(default)]
    pub quota_display_mode: QuotaDisplayMode,
    /// 每个 Provider 中被隐藏的配额标签集合（不在托盘弹窗中显示）
    /// key = provider id_key (如 "claude"), value = 隐藏的 quota label 集合
    #[serde(default)]
    pub hidden_quotas: HashMap<String, HashSet<String>>,
}

fn default_true() -> bool {
    true
}

fn default_language() -> String {
    "system".to_string()
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: AppTheme::Dark,
            refresh_interval_mins: 5,
            global_hotkey: "Cmd+Shift+S".to_string(),
            auto_hide_window: true,
            start_at_login: false,
            session_quota_notifications: true,
            notification_sound: true,
            providers: ProviderSettings::default(),
            enabled_providers: HashMap::new(),
            provider_order: Vec::new(),
            language: default_language(),
            show_dashboard_button: true,
            show_refresh_button: true,
            show_debug_tab: false,
            show_account_info: true,
            tray_icon_style: TrayIconStyle::default(),
            quota_display_mode: QuotaDisplayMode::default(),
            hidden_quotas: HashMap::new(),
        }
    }
}

impl AppSettings {
    /// 检查指定 Provider 是否已启用
    pub fn is_enabled(&self, id: &ProviderId) -> bool {
        self.enabled_providers
            .get(&id.id_key())
            .copied()
            .unwrap_or(false)
    }

    /// 设置指定 Provider 的启用状态
    pub fn set_provider_enabled(&mut self, kind: ProviderKind, enabled: bool) {
        self.enabled_providers
            .insert(kind.id_key().to_string(), enabled);
    }

    /// 通过 ProviderId 设置启用状态
    pub fn set_enabled(&mut self, id: &ProviderId, enabled: bool) {
        self.enabled_providers
            .insert(id.id_key().to_string(), enabled);
    }

    /// 按用户自定义顺序返回所有内置 Provider。未在 provider_order 中出现的 Provider 追加到末尾。
    pub fn ordered_providers(&self) -> Vec<ProviderKind> {
        let mut result = Vec::with_capacity(ProviderKind::all().len());
        let mut seen = HashSet::with_capacity(ProviderKind::all().len());

        // 先按保存的顺序添加
        for key in &self.provider_order {
            if let Some(kind) = ProviderKind::from_id_key(key) {
                if seen.insert(kind) {
                    result.push(kind);
                }
            }
        }

        // 再追加未出现的 Provider（保持默认顺序）
        for &kind in ProviderKind::all() {
            if seen.insert(kind) {
                result.push(kind);
            }
        }

        result
    }

    /// 按用户自定义顺序返回所有 Provider（内置 + 自定义）。
    /// `custom_ids` 为当前已加载的自定义 Provider ID 列表。
    pub fn ordered_provider_ids(&self, custom_ids: &[ProviderId]) -> Vec<ProviderId> {
        let mut result = Vec::new();
        let mut seen = HashSet::new();

        // 先按保存的顺序添加（内置 + 自定义均可能在 provider_order 中）
        for key in &self.provider_order {
            let id = ProviderId::from_id_key(key);
            if seen.insert(id.clone()) {
                result.push(id);
            }
        }

        // 追加未出现的内置 Provider（保持默认顺序）
        for &kind in ProviderKind::all() {
            let id = ProviderId::BuiltIn(kind);
            if seen.insert(id.clone()) {
                result.push(id);
            }
        }

        // 追加未出现的自定义 Provider
        for custom_id in custom_ids {
            if seen.insert(custom_id.clone()) {
                result.push(custom_id.clone());
            }
        }

        result
    }

    /// 将指定 Provider 在排序中上移一位。返回 true 表示发生了移动。
    pub fn move_provider_up(&mut self, id: &ProviderId, custom_ids: &[ProviderId]) -> bool {
        self.ensure_provider_order(custom_ids);
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
        self.ensure_provider_order(custom_ids);
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

    /// 统计可见配额数量（惰性求值，不分配内存）
    pub fn visible_quota_count(&self, kind: ProviderKind, quotas: &[QuotaInfo]) -> usize {
        quotas
            .iter()
            .filter(|q| self.is_quota_visible(kind, &q.quota_type.stable_key()))
            .count()
    }

    /// 过滤出在托盘弹窗中可见的配额（不包含被隐藏的项）
    /// 返回引用，避免不必要的 clone；调用方按需 `.cloned().collect()` 即可。
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
    /// `quota_key` 应使用 `QuotaType::stable_key()`，而非 i18n label
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
    fn ensure_provider_order(&mut self, custom_ids: &[ProviderId]) {
        self.provider_order = self
            .ordered_provider_ids(custom_ids)
            .into_iter()
            .map(|id| id.id_key().to_string())
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordered_providers_ignores_invalid_and_duplicate_keys() {
        let mut settings = AppSettings::default();
        settings.provider_order = vec![
            "gemini".into(),
            "invalid".into(),
            "claude".into(),
            "gemini".into(),
        ];

        let ordered = settings.ordered_providers();

        assert_eq!(ordered[0], ProviderKind::Gemini);
        assert_eq!(ordered[1], ProviderKind::Claude);
        assert_eq!(ordered.len(), ProviderKind::all().len());
    }

    #[test]
    fn move_provider_up_normalizes_provider_order() {
        let mut settings = AppSettings::default();
        // 初始顺序: gemini, claude（只用合法内置 key，避免混入 custom id）
        settings.provider_order = vec!["gemini".into(), "gemini".into(), "claude".into()];

        let claude = ProviderId::BuiltIn(ProviderKind::Claude);
        assert!(settings.move_provider_up(&claude, &[]));
        // ensure_provider_order 去重后：gemini, claude, ...rest
        // move_up claude: claude, gemini, ...rest
        assert_eq!(settings.provider_order[0], ProviderKind::Claude.id_key());
        assert_eq!(settings.provider_order[1], ProviderKind::Gemini.id_key());
        assert_eq!(settings.provider_order.len(), ProviderKind::all().len());
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

    #[test]
    fn app_settings_missing_tray_icon_style_defaults_to_monochrome() {
        // Simulate loading settings JSON that was saved before tray_icon_style existed.
        let json = serde_json::json!({
            "theme": "Dark",
            "refresh_interval_mins": 5,
            "global_hotkey": "Cmd+Shift+S",
            "auto_hide_window": true,
            "providers": {}
        });
        let settings: AppSettings = serde_json::from_value(json).unwrap();
        assert_eq!(settings.tray_icon_style, TrayIconStyle::Monochrome);
    }

    // ── hidden_quotas ────────────────────────────────────

    #[test]
    fn hidden_quotas_default_all_visible() {
        let settings = AppSettings::default();
        // 使用 QuotaType::stable_key() 而非 i18n label
        assert!(settings.is_quota_visible(ProviderKind::Claude, "session"));
        assert!(settings.is_quota_visible(ProviderKind::Claude, "model:Opus"));
    }

    #[test]
    fn toggle_quota_visibility_hides_then_shows() {
        let mut settings = AppSettings::default();
        assert!(settings.is_quota_visible(ProviderKind::Claude, "model:Opus"));

        settings.toggle_quota_visibility(ProviderKind::Claude, "model:Opus".to_string());
        assert!(!settings.is_quota_visible(ProviderKind::Claude, "model:Opus"));
        // 其他 key 不受影响
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

    // ── ordered_provider_ids ──────────────────────────────────

    #[test]
    fn ordered_provider_ids_respects_saved_order() {
        let mut settings = AppSettings::default();
        settings.provider_order = vec!["gemini".into(), "claude".into()];

        let ids = settings.ordered_provider_ids(&[]);
        assert_eq!(ids[0], ProviderId::BuiltIn(ProviderKind::Gemini));
        assert_eq!(ids[1], ProviderId::BuiltIn(ProviderKind::Claude));
        // 所有内置 Provider 都应出现
        assert!(ids.len() >= ProviderKind::all().len());
    }

    #[test]
    fn ordered_provider_ids_includes_custom() {
        let mut settings = AppSettings::default();
        settings.provider_order = vec!["gemini".into(), "myai:cli".into(), "claude".into()];
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
        // 自定义 Provider 应出现在最后
        assert!(ids.contains(&ProviderId::Custom("new:provider".to_string())));
        assert_eq!(ids.len(), ProviderKind::all().len() + 1);
    }

    #[test]
    fn ordered_provider_ids_deduplicates() {
        let mut settings = AppSettings::default();
        settings.provider_order = vec!["claude".into(), "claude".into(), "gemini".into()];

        let ids = settings.ordered_provider_ids(&[]);
        let claude_count = ids
            .iter()
            .filter(|id| **id == ProviderId::BuiltIn(ProviderKind::Claude))
            .count();
        assert_eq!(claude_count, 1);
    }

    // ── move_provider_up/down with ProviderId ─────────────────

    #[test]
    fn move_provider_down_works() {
        let mut settings = AppSettings::default();
        settings.provider_order = vec!["claude".into(), "gemini".into()];

        let claude = ProviderId::BuiltIn(ProviderKind::Claude);
        assert!(settings.move_provider_down(&claude, &[]));
        // claude 应该到 gemini 后面
        let pos_claude = settings
            .provider_order
            .iter()
            .position(|k| k == "claude")
            .unwrap();
        let pos_gemini = settings
            .provider_order
            .iter()
            .position(|k| k == "gemini")
            .unwrap();
        assert!(pos_gemini < pos_claude);
    }

    #[test]
    fn move_provider_up_at_top_returns_false() {
        let mut settings = AppSettings::default();
        settings.provider_order = vec!["claude".into(), "gemini".into()];

        let claude = ProviderId::BuiltIn(ProviderKind::Claude);
        assert!(!settings.move_provider_up(&claude, &[]));
    }

    #[test]
    fn move_provider_down_at_bottom_returns_false() {
        let mut settings = AppSettings::default();
        // ensure_provider_order 会填充所有内置 Provider，最后一个不能再下移
        let all_keys: Vec<String> = ProviderKind::all()
            .iter()
            .map(|k| k.id_key().to_string())
            .collect();
        settings.provider_order = all_keys;

        let last = ProviderKind::all().last().unwrap();
        let id = ProviderId::BuiltIn(*last);
        assert!(!settings.move_provider_down(&id, &[]));
    }

    #[test]
    fn move_custom_provider_up() {
        let mut settings = AppSettings::default();
        let custom = ProviderId::Custom("myai:cli".to_string());
        // 手动设置顺序：claude, myai:cli
        settings.provider_order = vec!["claude".into(), "myai:cli".into()];

        assert!(settings.move_provider_up(&custom, &[custom.clone()]));
        assert_eq!(settings.provider_order[0], "myai:cli");
        assert_eq!(settings.provider_order[1], "claude");
    }
}
