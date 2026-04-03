use super::provider::ProviderKind;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

// ============================================================================
// 应用设置
// ============================================================================

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
        }
    }
}

impl AppSettings {
    /// 检查指定 Provider 是否已启用
    pub fn is_provider_enabled(&self, kind: ProviderKind) -> bool {
        self.enabled_providers
            .get(kind.id_key())
            .copied()
            .unwrap_or(false)
    }

    /// 设置指定 Provider 的启用状态
    pub fn set_provider_enabled(&mut self, kind: ProviderKind, enabled: bool) {
        self.enabled_providers
            .insert(kind.id_key().to_string(), enabled);
    }

    /// 按用户自定义顺序返回所有 Provider。未在 provider_order 中出现的 Provider 追加到末尾。
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

    /// 将指定 Provider 在排序中上移一位。返回 true 表示发生了移动。
    pub fn move_provider_up(&mut self, kind: ProviderKind) -> bool {
        self.ensure_provider_order();
        let key = kind.id_key();
        if let Some(pos) = self.provider_order.iter().position(|k| k == key) {
            if pos > 0 {
                self.provider_order.swap(pos, pos - 1);
                return true;
            }
        }
        false
    }

    /// 将指定 Provider 在排序中下移一位。返回 true 表示发生了移动。
    pub fn move_provider_down(&mut self, kind: ProviderKind) -> bool {
        self.ensure_provider_order();
        let key = kind.id_key();
        if let Some(pos) = self.provider_order.iter().position(|k| k == key) {
            if pos + 1 < self.provider_order.len() {
                self.provider_order.swap(pos, pos + 1);
                return true;
            }
        }
        false
    }

    /// 确保 provider_order 包含所有 Provider
    fn ensure_provider_order(&mut self) {
        self.provider_order = self
            .ordered_providers()
            .into_iter()
            .map(|kind| kind.id_key().to_string())
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
        settings.provider_order = vec![
            "invalid".into(),
            "gemini".into(),
            "gemini".into(),
            "claude".into(),
        ];

        assert!(settings.move_provider_up(ProviderKind::Claude));
        assert_eq!(settings.provider_order[0], ProviderKind::Claude.id_key());
        assert_eq!(settings.provider_order[1], ProviderKind::Gemini.id_key());
        assert_eq!(settings.provider_order.len(), ProviderKind::all().len());
    }
}
