pub mod amp;
pub mod claude;
pub mod copilot;
pub mod gemini;
pub mod manager;

use crate::models::{ProviderKind, QuotaInfo};
use anyhow::Result;
use async_trait::async_trait;

pub use manager::ProviderManager;

/// AI Provider 的核心接口
#[async_trait]
pub trait AiProvider: Send + Sync {
    /// 该 Provider 的内部唯一标识
    fn id(&self) -> &'static str;

    /// 关联的枚举类型
    fn kind(&self) -> ProviderKind;

    /// 是否可以在当前设备或环境中可用（如 CLI 是否已安装等）
    async fn is_available(&self) -> bool {
        true
    }

    /// 核心方法：拉取最新的配额/用量情况
    async fn refresh(&self) -> Result<Vec<QuotaInfo>>;
}
