//! AI Provider 核心接口定义。

use crate::models::{
    AppSettings, ProviderCapability, ProviderDescriptor, RefreshData, SettingsCapability,
    TokenInputState,
};
use async_trait::async_trait;

use super::{default_token_input_state, ProviderError, ProviderResult};

/// AI Provider 的核心接口
#[async_trait]
pub trait AiProvider: Send + Sync {
    /// 获取 Provider 的描述符（ID + 元数据）
    fn descriptor(&self) -> ProviderDescriptor;

    /// 检查当前环境是否满足刷新条件。
    async fn check_availability(&self) -> ProviderResult<()> {
        Ok(())
    }

    /// 核心方法：拉取最新的配额/用量情况。
    ///
    /// 默认返回 `NoData`；`Monitorable` provider 必须覆盖此方法。
    /// `Placeholder` / `Informational` provider 无需实现。
    async fn refresh(&self) -> ProviderResult<RefreshData> {
        Err(ProviderError::NoData)
    }

    /// 声明该 Provider 的设置 UI 能力（默认无交互设置）
    ///
    /// 返回 `SettingsCapability::TokenInput` 即可让 Settings UI 自动显示
    /// Token 输入面板，无需在 selector 或 UI 层硬编码。
    fn settings_capability(&self) -> SettingsCapability {
        SettingsCapability::None
    }

    /// 声明 provider 的能力层级。
    ///
    /// `Monitorable` 参与正常刷新链路；
    /// `Informational` / `Placeholder` 只作为说明入口展示，不参与常规刷新。
    fn provider_capability(&self) -> ProviderCapability {
        ProviderCapability::Monitorable
    }

    /// 解析 TokenInput 面板的运行时展示状态。
    ///
    /// 默认行为：若 provider 声明了 `SettingsCapability::TokenInput`，
    /// 则仅从 settings 中读取该 credential 的当前值。
    fn resolve_token_input_state(&self, settings: &AppSettings) -> Option<TokenInputState> {
        match self.settings_capability() {
            SettingsCapability::TokenInput(config) => {
                Some(default_token_input_state(settings, config.credential_key))
            }
            _ => None,
        }
    }

    /// 同步 BananaTray 自己持久化的 provider credentials 到 provider 运行时。
    ///
    /// Provider 默认不需要此钩子；使用 `TokenInput` 且刷新路径依赖本地 override 的
    /// provider 可在内部保存线程安全快照，供后台刷新线程读取。
    fn sync_provider_credentials(&self, _credentials: &crate::models::ProviderSettings) {}
}
