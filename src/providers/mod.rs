pub mod manager;

use crate::models::{ProviderKind, ProviderMetadata, QuotaInfo};
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;

pub use manager::ProviderManager;

/// 消除零字段 Provider 的重复样板代码（struct + Default + new）
macro_rules! define_unit_provider {
    ($name:ident) => {
        pub struct $name;

        impl Default for $name {
            fn default() -> Self {
                Self
            }
        }

        impl $name {
            pub fn new() -> Self {
                Self
            }
        }
    };
}
pub(crate) use define_unit_provider;

// ============================================================================
// Provider 错误分类（替代字符串匹配，符合 OCP）
// ============================================================================

/// Provider 刷新失败的结构化错误类型
#[derive(Debug)]
pub enum ProviderError {
    /// Provider 在当前环境不可用（CLI 未安装、文件不存在等）
    Unavailable(String),
    /// 需要认证（token 过期、未登录等）
    AuthRequired(String),
    /// 需要配置（缺少环境变量、配置文件等）
    ConfigMissing(String),
    /// 网络或 API 调用失败
    FetchFailed(String),
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable(msg) => write!(f, "{}", msg),
            Self::AuthRequired(msg) => write!(f, "{}", msg),
            Self::ConfigMissing(msg) => write!(f, "{}", msg),
            Self::FetchFailed(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for ProviderError {}

impl ProviderError {
    /// 从 anyhow::Error 推断错误类型（向后兼容已有 provider 实现）
    pub fn classify(err: &anyhow::Error) -> Self {
        if let Some(provider_error) = err.downcast_ref::<Self>() {
            return match provider_error {
                Self::Unavailable(message) => Self::Unavailable(message.clone()),
                Self::AuthRequired(message) => Self::AuthRequired(message.clone()),
                Self::ConfigMissing(message) => Self::ConfigMissing(message.clone()),
                Self::FetchFailed(message) => Self::FetchFailed(message.clone()),
            };
        }

        let msg = err.to_string();
        let lower = msg.to_lowercase();
        if lower.contains("unavailable") || lower.contains("not found") {
            Self::Unavailable(msg)
        } else if lower.contains("authentication")
            || lower.contains("not logged in")
            || lower.contains("token expired")
            || lower.contains("re-authenticate")
            || lower.contains("session expired")
            || lower.contains("session cookie expired")
        {
            Self::AuthRequired(msg)
        } else if lower.contains("missing environment variable") || lower.contains("not configured")
        {
            Self::ConfigMissing(msg)
        } else {
            Self::FetchFailed(msg)
        }
    }
}

/// AI Provider 的核心接口
#[async_trait]
pub trait AiProvider: Send + Sync {
    /// 获取 Provider 的元数据
    fn metadata(&self) -> ProviderMetadata;

    /// 该 Provider 的内部唯一标识（通常与 kind().id_key() 不同，用于更细粒度的区分实现）
    fn id(&self) -> &'static str;

    /// 关联的枚举类型
    fn kind(&self) -> ProviderKind {
        self.metadata().kind
    }

    /// 是否可以在当前设备或环境中可用（如 CLI 是否已安装等）
    async fn is_available(&self) -> bool {
        true
    }

    /// 核心方法：拉取最新的配额/用量情况
    async fn refresh(&self) -> Result<Vec<QuotaInfo>>;
}

macro_rules! register_providers {
    ($($mod_name:ident => $struct_name:ident),* $(,)?) => {
        $(pub mod $mod_name;)*

        /// 注册所有可用的 Provider 实现
        pub fn register_all(manager: &mut ProviderManager) {
            $(
                manager.register(Arc::new($mod_name::$struct_name::new()));
            )*
        }
    };
}

register_providers!(
    amp => AmpProvider,
    claude => ClaudeProvider,
    codex => CodexProvider,
    copilot => CopilotProvider,
    cursor => CursorProvider,
    gemini => GeminiProvider,
    kilo => KiloProvider,
    kimi => KimiProvider,
    kiro => KiroProvider,
    minimax => MiniMaxProvider,
    opencode => OpenCodeProvider,
    vertex_ai => VertexAiProvider,
);
