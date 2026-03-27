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
    Cursor,
    OpenCode,
    MiniMax,
    VertexAi,
    Kilo,
    Kiro,
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
            Self::Cursor => "Cursor",
            Self::OpenCode => "OpenCode",
            Self::MiniMax => "MiniMax",
            Self::VertexAi => "Vertex AI",
            Self::Kilo => "Kilo",
            Self::Kiro => "Kiro",
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
            Self::Cursor => "src/icons/provider-cursor.svg",
            Self::OpenCode => "src/icons/provider-opencode.svg",
            Self::MiniMax => "src/icons/provider-minimax.svg",
            Self::VertexAi => "src/icons/provider-vertexai.svg",
            Self::Kilo => "src/icons/provider-kilo.svg",
            Self::Kiro => "src/icons/provider-kiro.svg",
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
            Self::Cursor => "Cursor account",
            Self::OpenCode => "OpenCode account",
            Self::MiniMax => "MiniMax account",
            Self::VertexAi => "Google Cloud account",
            Self::Kilo => "Kilo account",
            Self::Kiro => "AWS account",
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
            Self::Cursor => "https://www.cursor.com/settings",
            Self::OpenCode => "https://opencode.ai",
            Self::MiniMax => "https://platform.minimaxi.com",
            Self::VertexAi => "https://console.cloud.google.com/vertex-ai",
            Self::Kilo => "https://kilo.dev",
            Self::Kiro => "https://kiro.dev",
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
            Self::Cursor,
            Self::OpenCode,
            Self::MiniMax,
            Self::VertexAi,
            Self::Kilo,
            Self::Kiro,
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
            Self::Cursor => "cursor",
            Self::OpenCode => "opencode",
            Self::MiniMax => "minimax",
            Self::VertexAi => "vertexai",
            Self::Kilo => "kilo",
            Self::Kiro => "kiro",
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
            Self::Cursor => "cursor api",
            Self::OpenCode => "opencode api",
            Self::MiniMax => "minimax api",
            Self::VertexAi => "vertex ai api",
            Self::Kilo => "kilo api",
            Self::Kiro => "kiro api",
        }
    }
}

/// 底部导航页签
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NavTab {
    Provider(ProviderKind),
    Settings,
}
