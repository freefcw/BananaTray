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

/// Provider 元数据
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderMetadata {
    pub kind: ProviderKind,
    pub display_name: String,
    pub brand_name: String,
    pub icon_asset: String,
    pub dashboard_url: String,
    pub account_hint: String,
    pub source_label: String,
}

impl ProviderKind {
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
    pub fn id_key(&self) -> String {
        format!("{:?}", self).to_lowercase()
    }
}

/// 底部导航页签
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NavTab {
    Provider(ProviderKind),
    Settings,
}
