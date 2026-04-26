use serde::{Deserialize, Serialize};
use std::borrow::Cow;

// ============================================================================
// ProviderId: 统一标识内置或自定义 Provider
// ============================================================================

/// Provider 统一标识符
///
/// 区分内置 Provider（通过 ProviderKind 标识）和自定义 Provider（通过字符串 ID 标识）
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProviderId {
    /// 内置 Provider
    BuiltIn(ProviderKind),
    /// 自定义 Provider（YAML 声明），格式为 "name:source"
    Custom(String),
}

impl ProviderId {
    /// 获取配置文件中使用的标识符
    pub fn id_key(&self) -> String {
        match self {
            ProviderId::BuiltIn(kind) => kind.id_key().to_string(),
            ProviderId::Custom(id) => id.clone(),
        }
    }

    /// 从标识符反查 ProviderId
    ///
    /// 内置 Provider 返回 BuiltIn，未知标识符返回 Custom
    pub fn from_id_key(key: &str) -> Self {
        ProviderKind::from_id_key(key)
            .map(ProviderId::BuiltIn)
            .unwrap_or_else(|| ProviderId::Custom(key.to_string()))
    }

    /// 判断是否为内置 Provider
    pub fn is_builtin(&self) -> bool {
        matches!(self, ProviderId::BuiltIn(_))
    }

    /// 判断是否为自定义 Provider
    pub fn is_custom(&self) -> bool {
        matches!(self, ProviderId::Custom(_))
    }

    /// 如果是内置 Provider，返回 Some(ProviderKind)，否则返回 None
    pub fn as_builtin(&self) -> Option<ProviderKind> {
        match self {
            ProviderId::BuiltIn(kind) => Some(*kind),
            ProviderId::Custom(_) => None,
        }
    }

    /// 获取 ProviderKind（如果是自定义 Provider 则返回 ProviderKind::Custom）
    pub fn kind(&self) -> ProviderKind {
        match self {
            ProviderId::BuiltIn(kind) => *kind,
            ProviderId::Custom(_) => ProviderKind::Custom,
        }
    }
}

impl std::fmt::Display for ProviderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.id_key())
    }
}

// ============================================================================
// Provider 类型定义
// ============================================================================

macro_rules! define_provider_kind {
    ($($variant:ident => $id:literal => $module:ident::$provider:ident),* $(,)?) => {
        /// 支持的 AI Provider 枚举
        ///
        /// 内置 Provider 由 crate 级 manifest 生成，`Custom` 用于 YAML 声明的自定义 Provider。
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub enum ProviderKind {
            $($variant,)*
            /// YAML 声明的自定义 Provider
            Custom,
        }

        impl ProviderKind {
            /// 获取所有内置 Provider（不含 Custom）
            pub fn all() -> &'static [ProviderKind] {
                &[$(Self::$variant),*]
            }

            /// 配置文件中使用的小写标识符
            pub fn id_key(self) -> &'static str {
                match self {
                    $(Self::$variant => $id,)*
                    Self::Custom => "custom",
                }
            }

            /// 从小写标识符反查 ProviderKind
            pub fn from_id_key(key: &str) -> Option<Self> {
                match key {
                    $($id => Some(Self::$variant),)*
                    _ => None,
                }
            }
        }
    };
}

crate::builtin_provider_manifest::builtin_provider_manifest!(define_provider_kind);

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

/// Provider 描述符
///
/// 将注册 ID 与展示元数据收敛到单一入口，避免 `id()/metadata()/kind()` 分散定义。
/// `id` 使用 `Cow` 以同时支持内置 Provider（`&'static str`）和自定义 Provider（`String`）。
#[derive(Debug, Clone, PartialEq)]
pub struct ProviderDescriptor {
    pub id: Cow<'static, str>,
    pub metadata: ProviderMetadata,
}

/// Provider 的能力层级。
///
/// 用于区分：
/// - 可直接监控配额的 provider
/// - 仅用于说明/引导的 informational entry
/// - 可被发现但当前无法监控的 placeholder entry
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderCapability {
    #[default]
    Monitorable,
    Informational,
    Placeholder,
}

impl ProviderCapability {
    /// 是否参与正常刷新链路（启动 / 周期 / 手动 / Debug）。
    pub fn supports_refresh(self) -> bool {
        matches!(self, Self::Monitorable)
    }
}

// ============================================================================
// Provider 设置 UI 能力声明（纯数据模型，不依赖 GPUI）
// ============================================================================

/// Token 输入型设置的编辑语义。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenEditMode {
    /// 当前 token 已存储在设置中，可直接编辑已有值。
    EditStored,
    /// 当前 token 来自外部来源或尚未配置，用户只能设置新的本地 override。
    SetNew,
}

/// Token 输入面板的运行时展示状态（纯数据，不依赖 UI 框架）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenInputState {
    pub has_token: bool,
    pub masked: Option<String>,
    pub source_i18n_key: Option<&'static str>,
    pub edit_mode: TokenEditMode,
}

/// Token 输入型设置的静态配置。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenInputCapability {
    /// credential 存取 key（对应 `ProviderSettings` 中的存储 key）
    pub credential_key: &'static str,
    /// 输入框 placeholder 的 i18n key
    pub placeholder_i18n_key: &'static str,
    /// 帮助提示文本的 i18n key
    pub help_tip_i18n_key: &'static str,
    /// 标题的 i18n key
    pub title_i18n_key: &'static str,
    /// 描述的 i18n key
    pub description_i18n_key: &'static str,
    /// Token 创建链接
    pub create_url: &'static str,
}

/// Provider 设置 UI 的能力声明
///
/// 由 `AiProvider::settings_capability()` 返回，UI 层据此直接渲染对应的交互面板。
///
/// 新增支持交互设置的 Provider 只需返回合适的变体，无需修改 selector / UI 代码（OCP）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum SettingsCapability {
    /// 无交互设置（自动管理型，默认）
    #[default]
    None,
    /// Token 输入型设置（如 Copilot GitHub Token）
    TokenInput(TokenInputCapability),
    /// NewAPI 型自定义 Provider — 显示「编辑配置」按钮
    NewApiEditable,
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

impl ProviderDescriptor {
    pub fn kind(&self) -> ProviderKind {
        self.metadata.kind
    }

    pub fn metadata(&self) -> &ProviderMetadata {
        &self.metadata
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
        assert_eq!(ProviderKind::Windsurf.id_key(), "windsurf");
    }

    #[test]
    fn test_id_keys_are_unique_and_round_trip() {
        let mut seen = std::collections::HashSet::new();
        for &kind in ProviderKind::all() {
            let id_key = kind.id_key();
            assert!(
                seen.insert(id_key),
                "Duplicate ProviderKind id_key: {id_key}"
            );
            assert_eq!(ProviderKind::from_id_key(id_key), Some(kind));
        }
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
        assert_eq!(
            ProviderKind::from_id_key("windsurf"),
            Some(ProviderKind::Windsurf)
        );
        assert_eq!(ProviderKind::from_id_key("unknown"), None);
    }
}

/// 底部导航页签
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NavTab {
    /// 总览面板：所有 Provider 配额概览
    Overview,
    Provider(ProviderId),
    Settings,
}
