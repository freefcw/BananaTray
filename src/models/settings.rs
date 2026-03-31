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
    pub visible_provider_count: usize,
    /// 开机自启动
    #[serde(default)]
    pub start_at_login: bool,
    /// 显示消费概览
    #[serde(default = "default_true")]
    pub show_cost_summary: bool,
    /// 检查 Provider 状态页
    #[serde(default = "default_true")]
    pub check_provider_status: bool,
    /// Session 配额变更通知
    #[serde(default = "default_true")]
    pub session_quota_notifications: bool,
    /// 通知是否带声音
    #[serde(default = "default_true")]
    pub notification_sound: bool,
    /// 工具栏显示 Dashboard 按钮
    #[serde(default = "default_true")]
    pub show_toolbar_dashboard: bool,
    /// 工具栏显示 Refresh 按钮
    #[serde(default = "default_true")]
    pub show_toolbar_refresh: bool,
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
}

fn default_true() -> bool {
    true
}

fn default_language() -> String {
    "system".to_string()
}

/// 支持的语言列表：(locale_code, display_name_key)
pub const SUPPORTED_LANGUAGES: &[(&str, &str)] = &[
    ("system", "lang.system"),
    ("en", "lang.en"),
    ("zh-CN", "lang.zh_CN"),
];

/// 将系统 locale 标准化为支持的 locale code
fn normalize_locale(raw: &str) -> &'static str {
    let lower = raw.to_lowercase().replace('_', "-");
    if lower.starts_with("zh") {
        "zh-CN"
    } else {
        "en"
    }
}

/// 根据语言设置初始化 i18n locale
pub fn apply_locale(language: &str) {
    let locale = if language == "system" {
        let sys = sys_locale::get_locale().unwrap_or_else(|| "en".to_string());
        normalize_locale(&sys)
    } else {
        // 验证是否为支持的语言，不支持则回退到 en
        if SUPPORTED_LANGUAGES
            .iter()
            .any(|(code, _)| *code == language)
            && language != "system"
        {
            // 返回精确匹配
            match language {
                "zh-CN" => "zh-CN",
                "en" => "en",
                _ => "en",
            }
        } else {
            "en"
        }
    };
    rust_i18n::set_locale(locale);
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: AppTheme::Light,
            refresh_interval_mins: 5,
            global_hotkey: "Cmd+Shift+S".to_string(),
            auto_hide_window: true,
            visible_provider_count: 4,
            start_at_login: false,
            show_cost_summary: true,
            check_provider_status: true,
            session_quota_notifications: true,
            notification_sound: true,
            show_toolbar_dashboard: true,
            show_toolbar_refresh: true,
            providers: ProviderSettings::default(),
            enabled_providers: HashMap::new(),
            provider_order: Vec::new(),
            language: default_language(),
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
