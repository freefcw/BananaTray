use serde::{Deserialize, Serialize};

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
}

/// 底部导航页签
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NavTab {
    Overview,
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
    /// 配额类型标签（如 "Session", "Daily", "Weekly"）
    pub label: String,
}

impl QuotaInfo {
    pub fn new(label: impl Into<String>, used: f64, limit: f64) -> Self {
        Self {
            used,
            limit,
            label: label.into(),
        }
    }

    /// 使用百分比 (0.0 - 100.0)
    pub fn percentage(&self) -> f64 {
        if self.limit <= 0.0 {
            return 0.0;
        }
        (self.used / self.limit * 100.0).min(100.0)
    }

    /// 状态等级：Green / Yellow / Red
    pub fn status_level(&self) -> StatusLevel {
        let pct = self.percentage();
        if pct < 60.0 {
            StatusLevel::Green
        } else if pct < 85.0 {
            StatusLevel::Yellow
        } else {
            StatusLevel::Red
        }
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
    /// 上次更新时间描述（如 "Updated just now"）
    pub last_updated_at: Option<String>,
    /// 最近一次刷新失败时的提示文案
    pub error_message: Option<String>,
}

impl ProviderStatus {
    /// 获取最高用量的状态等级（用于总览显示）
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

/// 应用配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub theme: AppTheme,
    pub refresh_interval_secs: u64,
    pub global_hotkey: String,
    pub auto_hide_window: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: AppTheme::Dark,
            refresh_interval_secs: 30,
            global_hotkey: "Cmd+Shift+S".to_string(),
            auto_hide_window: true,
        }
    }
}
