use serde::{Deserialize, Serialize};

// ============================================================================
// Provider 类型定义
// ============================================================================

macro_rules! define_provider_kind {
    ($($variant:ident),* $(,)?) => {
        /// 支持的 AI Provider 枚举
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub enum ProviderKind {
            $($variant),*
        }

        impl ProviderKind {
            /// 获取所有 Provider
            pub fn all() -> &'static [ProviderKind] {
                &[$(Self::$variant),*]
            }
        }
    };
}

define_provider_kind!(
    Claude, Gemini, Copilot, Codex, Kimi, Amp, Cursor, OpenCode, MiniMax, VertexAi, Kilo, Kiro,
);

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
    /// 配置文件中使用的小写标识符
    pub fn id_key(self) -> &'static str {
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

    pub fn from_id_key(key: &str) -> Option<Self> {
        match key {
            "claude" => Some(Self::Claude),
            "gemini" => Some(Self::Gemini),
            "copilot" => Some(Self::Copilot),
            "codex" => Some(Self::Codex),
            "kimi" => Some(Self::Kimi),
            "amp" => Some(Self::Amp),
            "cursor" => Some(Self::Cursor),
            "opencode" => Some(Self::OpenCode),
            "minimax" => Some(Self::MiniMax),
            "vertexai" => Some(Self::VertexAi),
            "kilo" => Some(Self::Kilo),
            "kiro" => Some(Self::Kiro),
            _ => None,
        }
    }
}

impl ProviderMetadata {
    /// 用于兜底场景的占位元数据，避免在多个调用点重复构造默认值。
    pub fn fallback(kind: ProviderKind) -> Self {
        let display_name = format!("{:?}", kind);
        Self {
            kind,
            display_name: display_name.clone(),
            brand_name: display_name,
            source_label: "unknown".to_string(),
            account_hint: "account".to_string(),
            icon_asset: "src/icons/provider-unknown.svg".to_string(),
            dashboard_url: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_kind_all_not_empty() {
        let all = ProviderKind::all();
        assert!(!all.is_empty());
        // 确保没有重复
        let mut seen = std::collections::HashSet::new();
        for kind in all {
            assert!(
                seen.insert(kind),
                "Duplicate ProviderKind in all(): {:?}",
                kind
            );
        }
    }

    #[test]
    fn test_id_key_format() {
        assert_eq!(ProviderKind::Claude.id_key(), "claude");
        assert_eq!(ProviderKind::Gemini.id_key(), "gemini");
        assert_eq!(ProviderKind::VertexAi.id_key(), "vertexai");
    }

    #[test]
    fn test_from_id_key() {
        assert_eq!(
            ProviderKind::from_id_key(ProviderKind::Codex.id_key()),
            Some(ProviderKind::Codex)
        );
        assert_eq!(
            ProviderKind::from_id_key(ProviderKind::OpenCode.id_key()),
            Some(ProviderKind::OpenCode)
        );
        assert_eq!(ProviderKind::from_id_key("unknown"), None);
    }
}

/// 底部导航页签
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NavTab {
    Provider(ProviderKind),
    Settings,
}
