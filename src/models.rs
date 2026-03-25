use serde::{Deserialize, Serialize};
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
    /// Provider 特定配置
    pub providers: ProviderSettings,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: AppTheme::Dark,
            refresh_interval_mins: 5,
            global_hotkey: "Cmd+Shift+S".to_string(),
            auto_hide_window: true,
            visible_provider_count: 4,
            providers: ProviderSettings::default(),
        }
    }
}
