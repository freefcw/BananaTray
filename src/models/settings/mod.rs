use super::provider::{ProviderId, ProviderKind};
use super::quota::QuotaInfo;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

// ============================================================================
// 子结构体定义（按语义职责分组）
// ============================================================================

/// 系统行为设置
///
/// 容器级 `#[serde(default)]`：缺失字段从 `SystemSettings::default()` 回填（而非
/// 字段类型零值——`auto_hide_window` 的语义默认是 true）。旧配置无需迁移，
/// 未来新增字段即使忘加字段级属性也不会导致整个文件反序列化失败。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SystemSettings {
    pub auto_hide_window: bool,
    /// 开机自启动
    #[serde(default)]
    pub start_at_login: bool,
    /// 自动刷新间隔（分钟），0 表示禁用自动刷新
    pub refresh_interval_mins: u64,
    #[serde(default = "default_global_hotkey")]
    pub global_hotkey: String,
}

impl Default for SystemSettings {
    fn default() -> Self {
        Self {
            auto_hide_window: true,
            start_at_login: false,
            refresh_interval_mins: Self::DEFAULT_REFRESH_INTERVAL_MINS,
            global_hotkey: default_global_hotkey(),
        }
    }
}

impl SystemSettings {
    /// 默认刷新间隔（分钟）— 供 RefreshScheduler 等模块引用，保持单一来源。
    pub const DEFAULT_REFRESH_INTERVAL_MINS: u64 = 5;
    /// 默认全局热键，使用可回读的持久化格式保存。
    #[cfg(target_os = "macos")]
    pub const DEFAULT_GLOBAL_HOTKEY: &'static str = "cmd-shift-s";
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    pub const DEFAULT_GLOBAL_HOTKEY: &'static str = "super-shift-s";
    #[cfg(target_os = "windows")]
    pub const DEFAULT_GLOBAL_HOTKEY: &'static str = "win-shift-s";
}

/// 日志轮转 / 清理配置（不在 UI 中暴露，仅 settings.json 持久化）。
///
/// 阈值默认值与 `platform::logging` 中的常量保持一致；缺失字段通过
/// 容器级 `#[serde(default)]` 回填，保证旧 settings.json 无需迁移即可加载。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct LoggingSettings {
    /// 单个日志文件大小上限（字节），超过即轮转。0 表示禁用轮转。
    #[serde(default = "default_log_max_bytes")]
    pub max_bytes: u64,
    /// 保留的轮转文件份数（不含当前活跃文件）。
    #[serde(default = "default_log_max_files")]
    pub max_files: usize,
}

impl Default for LoggingSettings {
    fn default() -> Self {
        Self {
            max_bytes: default_log_max_bytes(),
            max_files: default_log_max_files(),
        }
    }
}

/// 默认单个日志文件上限：5 MiB。
const DEFAULT_LOG_MAX_BYTES: u64 = 5 * 1024 * 1024;
/// 默认保留轮转份数（1 活跃 + 4 轮转 = 5 文件）。
const DEFAULT_LOG_MAX_FILES: usize = 4;

fn default_log_max_bytes() -> u64 {
    DEFAULT_LOG_MAX_BYTES
}

fn default_log_max_files() -> usize {
    DEFAULT_LOG_MAX_FILES
}

/// 通知设置（容器级 `#[serde(default)]` 语义见 `SystemSettings`）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
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

/// 显示/外观设置（容器级 `#[serde(default)]` 语义见 `SystemSettings`）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
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
    /// 是否在托盘弹窗显示 Overview 总览面板
    #[serde(default = "default_true")]
    pub show_overview: bool,
    #[serde(default, skip_serializing_if = "TrayPopupSettings::is_default")]
    pub tray_popup: TrayPopupSettings,
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
            show_overview: true,
            tray_popup: TrayPopupSettings::default(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct TrayPopupSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linux_last_position: Option<SavedWindowPosition>,
}

impl TrayPopupSettings {
    fn is_default(&self) -> bool {
        self.linux_last_position.is_none()
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct SavedWindowPosition {
    pub x: f32,
    pub y: f32,
}

/// Provider 在设置页中的用户偏好。
///
/// 数组位置就是 Provider 的排序，不再额外保存 order 字段。
///
/// `in_sidebar` 与 `enabled` 是两个用户可感知的维度，但有明确约束：
/// 启用 Provider 必须出现在 sidebar 中；隐藏 Provider 必须同时禁用。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderLayoutItem {
    /// 稳定的 Provider id_key；内置 Provider 来自 `ProviderKind::id_key()`。
    id: String,
    /// 是否出现在设置页 sidebar / Provider 导航中。
    #[serde(default)]
    in_sidebar: bool,
    /// 是否参与 Overview、托盘状态和后台刷新。
    #[serde(default)]
    enabled: bool,
}

impl ProviderLayoutItem {
    pub fn new(id: impl Into<String>, in_sidebar: bool, enabled: bool) -> Self {
        Self {
            id: id.into(),
            in_sidebar,
            enabled: in_sidebar && enabled,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn is_in_sidebar(&self) -> bool {
        self.in_sidebar
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

/// Provider 管理配置。
///
/// Provider 的布局和启用状态统一存储在 `provider_layout`；凭证与 quota 可见性仍然
/// 分别属于独立的数据域，不混入布局项。
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    /// BananaTray 自己管理的 Provider 凭证覆盖值（如 github_token / custom_token）。
    ///
    /// 注意：这不代表 Provider 的完整认证状态；部分 Provider 还会从外部配置文件、
    /// CLI 登录态或环境变量读取凭证。
    pub credentials: ProviderSettings,
    /// 有序的 Provider 用户偏好；数组顺序就是设置页和导航排序。
    pub provider_layout: Vec<ProviderLayoutItem>,
    /// 每个 Provider 中被隐藏的配额标签集合（不在托盘弹窗中显示）。
    /// key = provider id_key (如 "claude"), value = 隐藏的 quota label 集合。
    pub hidden_quotas: HashMap<String, HashSet<String>>,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            credentials: ProviderSettings::default(),
            provider_layout: Self::default_layout(),
            hidden_quotas: HashMap::new(),
        }
    }
}

// ── ProviderConfig 核心方法（启用/禁用/清理）──
impl ProviderConfig {
    /// 首次启动默认展示 Claude + Codex，但不自动开启后台监控。
    pub fn default_layout() -> Vec<ProviderLayoutItem> {
        [ProviderKind::Claude, ProviderKind::Codex]
            .into_iter()
            .map(|kind| ProviderLayoutItem::new(kind.id_key(), true, false))
            .collect()
    }

    /// 检查指定 Provider 是否已启用。
    pub fn is_enabled(&self, id: &ProviderId) -> bool {
        self.layout_item(id).is_some_and(|item| item.enabled)
    }

    /// 检查指定 Provider 是否出现在 sidebar。
    pub fn is_in_sidebar(&self, id: &ProviderId) -> bool {
        self.layout_item(id).is_some_and(|item| item.in_sidebar)
    }

    /// 检查指定 Provider 是否已经有用户配置记录。
    pub fn has_layout_item(&self, id: &ProviderId) -> bool {
        self.layout_item(id).is_some()
    }

    /// 设置指定 Provider 的启用状态（按 ProviderKind）。
    ///
    /// 已废弃：请使用 `set_enabled(&ProviderId::BuiltIn(kind), enabled)` 替代。
    #[deprecated(note = "use set_enabled(&ProviderId::BuiltIn(kind), enabled) instead")]
    pub fn set_provider_enabled(&mut self, kind: ProviderKind, enabled: bool) {
        self.set_enabled(&ProviderId::BuiltIn(kind), enabled);
    }

    /// 通过 ProviderId 设置启用状态。
    pub fn set_enabled(&mut self, id: &ProviderId, enabled: bool) {
        let item = self.ensure_layout_item(id);
        item.enabled = enabled;
        if enabled {
            item.in_sidebar = true;
        }
    }

    /// 删除一个已从磁盘移除的 Provider 的所有持久引用。
    pub fn remove_provider_references(&mut self, id: &ProviderId) {
        let key = id.id_key();
        self.provider_layout.retain(|item| item.id != key);
        self.hidden_quotas.remove(&key);
    }

    /// 清除已不存在的自定义 Provider ID（热重载后清理残留）。
    pub fn prune_stale_custom_ids(&mut self, existing_custom_ids: &[ProviderId]) -> bool {
        let existing: HashSet<String> = existing_custom_ids
            .iter()
            .filter_map(|id| match id {
                ProviderId::Custom(s) => Some(s.clone()),
                _ => None,
            })
            .collect();

        let before = self.provider_layout.len() + self.hidden_quotas.len();
        self.provider_layout.retain(|item| {
            ProviderKind::from_id_key(&item.id).is_some() || existing.contains(&item.id)
        });
        self.hidden_quotas
            .retain(|key, _| ProviderKind::from_id_key(key).is_some() || existing.contains(key));
        let normalized = self.normalize_layout();
        let after = self.provider_layout.len() + self.hidden_quotas.len();
        normalized || before != after
    }

    pub(crate) fn normalize_layout(&mut self) -> bool {
        let before = self.provider_layout.clone();
        let mut seen = HashSet::new();
        self.provider_layout.retain_mut(|item| {
            if item.id.is_empty() || !seen.insert(item.id.clone()) {
                return false;
            }
            if !item.in_sidebar {
                item.enabled = false;
            }
            true
        });
        before != self.provider_layout
    }

    pub(super) fn layout_item(&self, id: &ProviderId) -> Option<&ProviderLayoutItem> {
        let key = id.id_key();
        self.provider_layout.iter().find(|item| item.id == key)
    }

    pub(super) fn layout_item_mut(&mut self, id: &ProviderId) -> Option<&mut ProviderLayoutItem> {
        let key = id.id_key();
        self.provider_layout.iter_mut().find(|item| item.id == key)
    }

    pub(super) fn ensure_layout_item(&mut self, id: &ProviderId) -> &mut ProviderLayoutItem {
        let key = id.id_key();
        if !self.provider_layout.iter().any(|item| item.id == key) {
            self.provider_layout
                .push(ProviderLayoutItem::new(key, false, false));
        }
        self.provider_layout
            .iter_mut()
            .find(|item| item.id == id.id_key())
            .expect("provider layout item was inserted")
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrayIconStyle {
    /// 单色 — macOS template 模式，跟随系统深色/浅色自动适配
    Monochrome,
    /// 黄色香蕉
    Yellow,
    /// 多彩渐变色香蕉
    Colorful,
    /// 动态模式 — 根据所有已启用 Provider 的额度综合状态自动切换颜色
    /// Green 状态使用 Monochrome，Yellow/Red 状态使用对应彩色图标
    Dynamic,
}

// Linux 没有 template rendering，黑色 Monochrome 在深色面板上不可见，
// 默认使用 Yellow 确保首次启动图标可见。
// 两个 #[cfg] 分支各自返回不同变体，derive 无法表达这种平台差异。
#[cfg(target_os = "linux")]
#[allow(clippy::derivable_impls)]
impl Default for TrayIconStyle {
    fn default() -> Self {
        Self::Yellow
    }
}

#[cfg(not(target_os = "linux"))]
#[allow(clippy::derivable_impls)]
impl Default for TrayIconStyle {
    fn default() -> Self {
        Self::Monochrome
    }
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

/// Provider 设置中由 BananaTray 自己持久化的凭证存储
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ProviderSettings {
    /// Provider-specific credentials, flattened for backward-compatible JSON shape.
    #[serde(flatten)]
    entries: BTreeMap<String, String>,
}

impl ProviderSettings {
    /// 通过 credential_key 获取凭证值（对应 `SettingsCapability::TokenInput::credential_key`）
    pub fn get_credential(&self, key: &str) -> Option<&str> {
        self.entries.get(key).map(String::as_str)
    }

    /// 通过 credential_key 设置凭证值
    pub fn set_credential(&mut self, key: &str, value: String) {
        if key.trim().is_empty() {
            log::warn!(target: "settings", "empty credential key");
            return;
        }
        self.entries.insert(key.to_string(), value);
    }

    /// 删除指定 credential。
    pub fn remove_credential(&mut self, key: &str) -> bool {
        self.entries.remove(key).is_some()
    }
}

// ============================================================================
// 应用设置（顶层）
// ============================================================================

/// 应用运行时配置 — 按职责分为五组子设置。
///
/// 顶层磁盘格式由 `settings_store::PersistedAppSettingsV1` 负责；这里不直接派生
/// serde，避免领域模型与 settings.json 的版本演进绑定。
#[derive(Debug, Clone, Default)]
pub struct AppSettings {
    /// 系统行为：自动隐藏、开机自启、刷新间隔、全局热键
    pub system: SystemSettings,
    /// 通知：配额通知、通知声音
    pub notification: NotificationSettings,
    /// 显示/外观：主题、语言、托盘图标、各 UI 开关
    pub display: DisplaySettings,
    /// 日志：轮转 / 清理阈值（不在 UI 中暴露）
    pub logging: LoggingSettings,
    /// Provider 管理：启用状态、排序、隐藏配额、sidebar、以及 app-managed credentials
    pub provider: ProviderConfig,
}

fn default_true() -> bool {
    true
}

fn default_language() -> String {
    "system".to_string()
}

fn default_global_hotkey() -> String {
    SystemSettings::DEFAULT_GLOBAL_HOTKEY.to_string()
}

// ============================================================================
#[cfg(test)]
mod tests;
