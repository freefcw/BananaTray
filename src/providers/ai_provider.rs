//! AI Provider 核心接口定义。

use super::{default_token_input_state, ProviderError, ProviderResult};
use crate::models::{
    AppSettings, ProviderCapability, ProviderDescriptor, ProviderSettings, RefreshData,
    SettingsCapability, TokenInputState,
};
use async_trait::async_trait;

/// Provider 执行时的运行上下文。
pub struct ProviderExecutionContext<'a> {
    pub provider_credentials: &'a ProviderSettings,
}

/// AI Provider 的核心刷新接口。
#[async_trait]
pub trait AiProvider: Send + Sync {
    /// 获取 Provider 的描述符（ID + 元数据）
    fn descriptor(&self) -> ProviderDescriptor;

    /// 检查当前环境是否满足刷新条件。
    async fn check_availability(&self, _ctx: &ProviderExecutionContext<'_>) -> ProviderResult<()> {
        Ok(())
    }

    /// 核心方法：拉取最新的配额/用量情况。
    ///
    /// 默认返回 `NoData`；`Monitorable` provider 必须覆盖此方法。
    /// `Placeholder` / `Informational` provider 无需实现。
    async fn refresh(&self, _ctx: &ProviderExecutionContext<'_>) -> ProviderResult<RefreshData> {
        Err(ProviderError::NoData)
    }
}

/// Provider 的产品 / 设置能力适配器。
pub trait ProviderCapabilities: Send + Sync {
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
}

/// Provider 注册表中使用的完整条目类型。
pub trait ProviderEntry: AiProvider + ProviderCapabilities {}

impl<T> ProviderEntry for T where T: AiProvider + ProviderCapabilities {}
