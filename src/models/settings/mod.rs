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
    /// 设置页 sidebar 中展示的 Provider id_key 列表（动态子集）
    /// 空列表由 `ensure_sidebar_defaults()` 在启动时填充
    #[serde(default)]
    pub sidebar_providers: Vec<String>,
}

// ── ProviderConfig 核心方法（启用/禁用/清理）──
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

    /// 清除已不存在的自定义 Provider ID（热重载后清理残留）
    ///
    /// 从 `enabled_providers`、`provider_order`、`hidden_quotas`、`sidebar_providers` 中移除
    /// 不再存在的自定义 Provider 条目。返回 true 表示发生了变更。
    pub fn prune_stale_custom_ids(&mut self, existing_custom_ids: &[ProviderId]) -> bool {
        let existing: std::collections::HashSet<String> = existing_custom_ids
            .iter()
            .filter_map(|id| match id {
                ProviderId::Custom(s) => Some(s.clone()),
                _ => None,
            })
            .collect();

        let is_valid_key = |key: &String| -> bool {
            // 内置 Provider key 始终保留
            ProviderKind::from_id_key(key).is_some() || existing.contains(key)
        };

        let before = self.enabled_providers.len()
            + self.provider_order.len()
            + self.hidden_quotas.len()
            + self.sidebar_providers.len();

        self.enabled_providers.retain(|key, _| is_valid_key(key));
        self.provider_order.retain(|key| is_valid_key(key));
        self.hidden_quotas.retain(|key, _| is_valid_key(key));
        self.sidebar_providers.retain(|key| is_valid_key(key));

        let after = self.enabled_providers.len()
            + self.provider_order.len()
            + self.hidden_quotas.len()
            + self.sidebar_providers.len();
        before != after
    }
}

// ── ProviderConfig 领域方法（独立文件）──
mod provider_config_ordering;
mod provider_config_quota;
mod provider_config_sidebar;

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
    /// 动态模式 — 根据所有已启用 Provider 的额度综合状态自动切换颜色
    /// Green 状态使用 Monochrome，Yellow/Red 状态使用对应彩色图标
    Dynamic,
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

// ============================================================================
// 旧格式迁移（独立文件）
// ============================================================================

mod migration;

#[cfg(test)]
mod tests;
