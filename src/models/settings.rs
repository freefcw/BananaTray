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

    /// 将指定 Provider 移动到目标索引位置（拖拽排序）。返回 true 表示发生了移动。
    pub fn move_provider_to_index(
        &mut self,
        id: &ProviderId,
        target_index: usize,
        custom_ids: &[ProviderId],
    ) -> bool {
        self.ensure_order(custom_ids);
        let key = id.id_key();
        if let Some(current) = self.provider_order.iter().position(|k| *k == key) {
            let target = target_index.min(self.provider_order.len().saturating_sub(1));
            if current != target {
                let item = self.provider_order.remove(current);
                self.provider_order.insert(target, item);
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

    // ── sidebar 动态列表管理 ──

    /// 设置页 sidebar 应展示的 Provider ID 列表。
    ///
    /// 返回 `sidebar_providers` 中有效的 Provider，按 `provider_order` 排序；
    /// 不在 `sidebar_providers` 中的项不展示。
    pub fn sidebar_provider_ids(&self, custom_ids: &[ProviderId]) -> Vec<ProviderId> {
        let sidebar_set: HashSet<&str> =
            self.sidebar_providers.iter().map(|s| s.as_str()).collect();
        // 按 provider_order 的顺序，过滤出在 sidebar 中的项
        self.ordered_provider_ids(custom_ids)
            .into_iter()
            .filter(|id| sidebar_set.contains(id.id_key().as_str()))
            .collect()
    }

    /// 返回可添加到 sidebar 的内置 Provider 列表。
    ///
    /// 规则：全量内置 Provider 中排除已在 sidebar 中的（Custom 类型不在此列，
    /// NewAPI 有独立入口）。
    pub fn addable_provider_kinds(&self) -> Vec<ProviderKind> {
        let sidebar_set: HashSet<&str> =
            self.sidebar_providers.iter().map(|s| s.as_str()).collect();
        ProviderKind::all()
            .iter()
            .filter(|kind| !sidebar_set.contains(kind.id_key()))
            .copied()
            .collect()
    }

    /// 将 Provider 添加到 sidebar 列表。
    ///
    /// 内置 Provider 重复添加返回 false；Custom 类型始终允许。
    pub fn add_to_sidebar(&mut self, id: &ProviderId) -> bool {
        let key = id.id_key();
        // 内置 Provider 去重
        if id.is_builtin() && self.sidebar_providers.contains(&key) {
            return false;
        }
        self.sidebar_providers.push(key.clone());
        // 同步到 provider_order（排序列表也需要包含该项）
        if !self.provider_order.contains(&key) {
            self.provider_order.push(key);
        }
        true
    }

    /// 从 sidebar 列表移除 Provider。返回 true 表示移除成功。
    pub fn remove_from_sidebar(&mut self, id: &ProviderId) -> bool {
        let key = id.id_key();
        let before = self.sidebar_providers.len();
        self.sidebar_providers.retain(|k| *k != key);
        self.sidebar_providers.len() != before
    }

    /// 若 sidebar_providers 为空，填充默认值。
    ///
    /// - 新用户（enabled_providers 也为空）→ 默认 ["claude", "codex"]
    /// - 老用户（enabled_providers 非空但 sidebar 为空）→ 全量内置 + 已有自定义
    pub fn ensure_sidebar_defaults(&mut self, custom_ids: &[ProviderId]) {
        if !self.sidebar_providers.is_empty() {
            return;
        }
        if self.enabled_providers.is_empty() && self.provider_order.is_empty() {
            // 新用户
            self.sidebar_providers = vec![
                ProviderKind::Claude.id_key().to_string(),
                ProviderKind::Codex.id_key().to_string(),
            ];
        } else {
            // 老用户：保留全集（向后兼容，不丢失现有 Provider）
            self.sidebar_providers = self
                .ordered_provider_ids(custom_ids)
                .into_iter()
                .map(|id| id.id_key())
                .collect();
        }
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

    pub fn move_provider_to_index(
        &mut self,
        id: &ProviderId,
        target_index: usize,
        custom_ids: &[ProviderId],
    ) -> bool {
        self.provider
            .move_provider_to_index(id, target_index, custom_ids)
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
// 旧格式迁移（独立文件）
// ============================================================================

#[path = "settings_migration.rs"]
mod migration;

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
