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
