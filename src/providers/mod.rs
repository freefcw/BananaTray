pub mod manager;

use crate::models::{ProviderKind, ProviderMetadata, QuotaInfo};
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;

pub use manager::ProviderManager;

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
