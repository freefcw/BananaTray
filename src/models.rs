use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Instant;

// ============================================================================
// Provider 类型定义
// ============================================================================

/// 支持的 AI Provider 枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProviderKind {
    Claude,
    Gemini,
    Copilot,
    Codex,
    Kimi,
    Amp,
}

/// 配额类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuotaType {
    /// 5h 滑动窗口会话配额
    Session,
    /// 周配额（所有模型合计）
    Weekly,
    /// 按模型的周配额（如 Opus / Sonnet）
    ModelSpecific(String),
    /// 基于金额的信用额度
    Credit,
    /// 通用/不确定类型
    General,
}

impl ProviderKind {
    /// 获取 Provider 显示名称
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Claude => "Claude",
            Self::Gemini => "Gemini",
            Self::Copilot => "Copilot",
            Self::Codex => "Codex",
            Self::Kimi => "Kimi",
            Self::Amp => "Amp",
        }
    }

    pub fn icon_asset(&self) -> &'static str {
        match self {
            Self::Claude => "src/icons/provider-claude.svg",
            Self::Gemini => "src/icons/provider-gemini.svg",
            Self::Copilot => "src/icons/provider-copilot.svg",
            Self::Codex => "src/icons/provider-codex.svg",
            Self::Kimi => "src/icons/provider-kimi.svg",
            Self::Amp => "src/icons/provider-amp.svg",
        }
    }

    #[allow(dead_code)]
    pub fn account_hint(&self) -> &'static str {
        match self {
            Self::Claude => "Anthropic workspace",
            Self::Gemini => "Google account",
            Self::Copilot => "GitHub account",
            Self::Codex => "OpenAI account",
            Self::Kimi => "Moonshot account",
            Self::Amp => "Amp CLI",
        }
    }

    /// 获取 Provider 用量详情页面 URL
    pub fn dashboard_url(&self) -> &'static str {
        match self {
            Self::Claude => "https://console.anthropic.com/settings/usage",
            Self::Gemini => "https://aistudio.google.com/billing",
            Self::Copilot => "https://github.com/settings/copilot",
            Self::Codex => "https://platform.openai.com/usage",
            Self::Kimi => "https://platform.moonshot.cn/console/account",
            Self::Amp => "https://app.amphq.com/usage",
        }
    }

    /// 获取所有 Provider
    pub fn all() -> &'static [ProviderKind] {
        &[
            Self::Claude,
            Self::Gemini,
            Self::Copilot,
            Self::Codex,
            Self::Kimi,
            Self::Amp,
        ]
    }

    /// 配置文件中使用的小写标识符
    pub fn id_key(&self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Gemini => "gemini",
            Self::Copilot => "copilot",
            Self::Codex => "codex",
            Self::Kimi => "kimi",
            Self::Amp => "amp",
        }
    }

    /// 数据源描述标签
    pub fn source_label(&self) -> &'static str {
        match self {
            Self::Claude => "claude cli",
            Self::Gemini => "gemini api",
            Self::Copilot => "github api",
            Self::Codex => "openai api",
            Self::Kimi => "kimi api",
            Self::Amp => "amp cli",
        }
    }
}

/// 底部导航页签
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NavTab {
    Provider(ProviderKind),
    Settings,
}

// ============================================================================
// 用量信息
// ============================================================================

/// 用量配额信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaInfo {
    /// 已使用量
    pub used: f64,
    /// 总配额
    pub limit: f64,
    /// 配额类型标签（如 "Session (5h)", "Weekly", "Pro"）
    pub label: String,
    /// 配额类型
    #[serde(default = "default_quota_type")]
    pub quota_type: QuotaType,
    /// 重置时间描述（如 "Resets in 2h 15m"）
    pub reset_at: Option<String>,
}

fn default_quota_type() -> QuotaType {
    QuotaType::General
}

impl QuotaInfo {
    pub fn new(label: impl Into<String>, used: f64, limit: f64) -> Self {
        Self {
            used,
            limit,
            label: label.into(),
            quota_type: QuotaType::General,
            reset_at: None,
        }
    }

    /// 创建带完整信息的配额
    pub fn with_details(
        label: impl Into<String>,
        used: f64,
        limit: f64,
        quota_type: QuotaType,
        reset_at: Option<String>,
    ) -> Self {
        Self {
            used,
            limit,
            label: label.into(),
            quota_type,
            reset_at,
        }
    }

    /// 使用百分比 (0.0 - 100.0)
    pub fn percentage(&self) -> f64 {
        if self.limit <= 0.0 {
            return 0.0;
        }
        (self.used / self.limit * 100.0).min(100.0)
    }

    /// 是否是纯百分比模式（limit == 100.0，数据本身就是百分比）
    #[allow(dead_code)]
    pub fn is_percentage_mode(&self) -> bool {
        (self.limit - 100.0).abs() < f64::EPSILON
    }

    /// 状态等级：Green / Yellow / Red (基于剩余量)
    pub fn status_level(&self) -> StatusLevel {
        let pct = self.percentage();
        let remaining_pct = (100.0 - pct).max(0.0);

        if remaining_pct > 50.0 {
            StatusLevel::Green
        } else if remaining_pct >= 20.0 {
            StatusLevel::Yellow
        } else {
            StatusLevel::Red
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quota_percentage() {
        let q1 = QuotaInfo::new("test", 50.0, 100.0);
        assert_eq!(q1.percentage(), 50.0);

        let q2 = QuotaInfo::new("test", 150.0, 100.0); // 溢出
        assert_eq!(q2.percentage(), 100.0);

        let q3 = QuotaInfo::new("test", 0.0, 0.0); // 除零
        assert_eq!(q3.percentage(), 0.0);
    }

    #[test]
    fn test_quota_status_level() {
        // 阈值：>50% Green, 20-50% Yellow, <20% Red (基于剩余值)

        // 剩余 60% (已用 40%) -> Green
        let q_green = QuotaInfo::new("green", 40.0, 100.0);
        assert_eq!(q_green.status_level(), StatusLevel::Green);

        // 剩余 50% (已用 50%) -> Yellow (刚好在 50% 边界，不大于 50%)
        let q_yellow_edge = QuotaInfo::new("yellow", 50.0, 100.0);
        assert_eq!(q_yellow_edge.status_level(), StatusLevel::Yellow);

        // 剩余 20% (已用 80%) -> Yellow (刚好在 20% 边界)
        let q_yellow_20 = QuotaInfo::new("yellow", 80.0, 100.0);
        assert_eq!(q_yellow_20.status_level(), StatusLevel::Yellow);

        // 剩余 19% (已用 81%) -> Red
        let q_red = QuotaInfo::new("red", 81.0, 100.0);
        assert_eq!(q_red.status_level(), StatusLevel::Red);
    }
}

// ============================================================================
// 状态等级
// ============================================================================

/// 用量状态等级（用于颜色编码）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusLevel {
    Green,
    Yellow,
    Red,
}

// ============================================================================
// Provider 状态
// ============================================================================

/// 连接状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionStatus {
    Connected,
    Disconnected,
    Refreshing,
    Error,
}

/// 单个 Provider 的完整状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderStatus {
    pub kind: ProviderKind,
    pub enabled: bool,
    pub connection: ConnectionStatus,
    pub quotas: Vec<QuotaInfo>,
    /// 账号邮箱（可选，用于 UI 展示）
    pub account_email: Option<String>,
    /// 是否为付费版
    pub is_paid: bool,
    /// 账号层级（如 "Pro", "Max", "Free", "Business"）
    pub account_tier: Option<String>,
    /// 上次更新时间描述（仅用于错误/断连状态的静态文案）
    pub last_updated_at: Option<String>,
    /// 最近一次刷新失败时的提示文案
    pub error_message: Option<String>,
    /// 上次成功刷新的时刻（不序列化，用于计算相对时间）
    #[serde(skip)]
    pub last_refreshed_instant: Option<Instant>,
}

impl ProviderStatus {
    /// 格式化上次刷新的相对时间
    pub fn format_last_updated(&self) -> String {
        if let Some(instant) = self.last_refreshed_instant {
            let secs = instant.elapsed().as_secs();
            if secs < 60 {
                "Updated just now".to_string()
            } else if secs < 3600 {
                format!("Updated {} min ago", secs / 60)
            } else {
                format!("Updated {} hr ago", secs / 3600)
            }
        } else if let Some(ref text) = self.last_updated_at {
            text.clone()
        } else {
            match self.connection {
                ConnectionStatus::Connected => "Waiting for data".to_string(),
                ConnectionStatus::Refreshing => "Refreshing…".to_string(),
                ConnectionStatus::Error => "Needs attention".to_string(),
                ConnectionStatus::Disconnected => "Not connected".to_string(),
            }
        }
    }

    /// 获取最高用量的状态等级（用于总览显示）
    #[allow(dead_code)]
    pub fn worst_status(&self) -> StatusLevel {
        self.quotas
            .iter()
            .map(|q| q.status_level())
            .max_by_key(|s| match s {
                StatusLevel::Green => 0,
                StatusLevel::Yellow => 1,
                StatusLevel::Red => 2,
            })
            .unwrap_or(StatusLevel::Green)
    }
}

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
    /// Provider 特定配置
    pub providers: ProviderSettings,
    /// 各 Provider 启用状态（key = provider id_key, value = enabled）
    #[serde(default)]
    pub enabled_providers: HashMap<String, bool>,
}

fn default_true() -> bool {
    true
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
            providers: ProviderSettings::default(),
            enabled_providers: HashMap::new(),
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
}

// ============================================================================
// 弹出窗口布局常量与计算
// ============================================================================

/// 弹出窗口布局相关常量，集中管理避免 magic numbers
pub struct PopupLayout;

impl PopupLayout {
    /// 弹出窗口固定宽度（px）
    pub const WIDTH: f32 = 308.0;
    /// 基础高度：nav_bar(~46) + header(~40) + menu(~110) + padding(~44)
    pub const BASE_HEIGHT: f32 = 240.0;
    /// 每个 quota bar 的预估高度
    pub const PER_QUOTA_HEIGHT: f32 = 42.0;
    /// 最小窗口高度
    pub const MIN_HEIGHT: f32 = 300.0;
    /// 最大窗口高度
    pub const MAX_HEIGHT: f32 = 548.0;
}

/// 根据 quota 数量计算弹出窗口高度（纯函数，适合测试）
pub fn compute_popup_height_for_quotas(quota_count: usize) -> f32 {
    let count = quota_count.max(1) as f32;
    (PopupLayout::BASE_HEIGHT + count * PopupLayout::PER_QUOTA_HEIGHT)
        .clamp(PopupLayout::MIN_HEIGHT, PopupLayout::MAX_HEIGHT)
}

#[cfg(test)]
mod layout_tests {
    use super::*;

    #[test]
    fn test_popup_height_clamps_to_minimum() {
        // 0 个 quota → max(1) → 240 + 42 = 282 → clamp 到 300
        assert_eq!(compute_popup_height_for_quotas(0), PopupLayout::MIN_HEIGHT);
    }

    #[test]
    fn test_popup_height_single_quota() {
        // 1 个 quota: 240 + 42 = 282 → clamp 到 300
        assert_eq!(compute_popup_height_for_quotas(1), PopupLayout::MIN_HEIGHT);
    }

    #[test]
    fn test_popup_height_three_quotas() {
        // 3 个 quota: 240 + 3*42 = 366
        let height = compute_popup_height_for_quotas(3);
        assert!((height - 366.0).abs() < f32::EPSILON);
        assert!(height >= PopupLayout::MIN_HEIGHT);
        assert!(height <= PopupLayout::MAX_HEIGHT);
    }

    #[test]
    fn test_popup_height_clamps_to_maximum() {
        // 20 个 quota: 240 + 20*42 = 1080 → clamp 到 548
        assert_eq!(compute_popup_height_for_quotas(20), PopupLayout::MAX_HEIGHT);
    }

    #[test]
    fn test_popup_height_monotonically_increases() {
        let mut prev = compute_popup_height_for_quotas(1);
        for n in 2..=8 {
            let h = compute_popup_height_for_quotas(n);
            assert!(h >= prev, "height should be non-decreasing");
            prev = h;
        }
    }
}
