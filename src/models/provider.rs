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
    pub fn id_key(&self) -> String {
        format!("{:?}", self).to_lowercase()
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
}

/// 底部导航页签
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NavTab {
    Provider(ProviderKind),
    Settings,
}
