//! 共享测试工具函数
//!
//! 统一 ProviderStatus / ProviderMetadata 的构造逻辑，
//! 消除 quota.rs / application/state.rs / provider_logic.rs / selectors.rs 中的重复定义。

use super::provider::{ProviderCapability, ProviderId, ProviderKind, ProviderMetadata};
use super::quota::{ConnectionStatus, ProviderStatus};

/// 创建测试用的 ProviderMetadata
pub fn make_test_metadata(kind: ProviderKind) -> ProviderMetadata {
    ProviderMetadata {
        kind,
        display_name: format!("{:?}", kind),
        brand_name: format!("{:?}", kind),
        source_label: "test".to_string(),
        account_hint: "test account".to_string(),
        icon_asset: "src/icons/provider.svg".to_string(),
        dashboard_url: "https://example.com".to_string(),
    }
}

/// 创建测试用的 ProviderStatus（指定连接状态）
///
/// 自动为 Copilot 设置 `TokenInput` capability（与 `CopilotProvider::settings_capability()` 一致），
/// 保证测试中 settings capability 的行为与生产环境对齐。
pub fn make_test_provider(kind: ProviderKind, connection: ConnectionStatus) -> ProviderStatus {
    let mut status = ProviderStatus::new(ProviderId::BuiltIn(kind), make_test_metadata(kind));
    status.connection = connection;
    // 与 CopilotProvider::settings_capability() 保持一致
    if kind == ProviderKind::Copilot {
        status.settings_capability = crate::providers::copilot_settings_capability();
    }
    status.provider_capability = match kind {
        ProviderKind::VertexAi => ProviderCapability::Informational,
        ProviderKind::Kilo | ProviderKind::OpenCode => ProviderCapability::Placeholder,
        _ => ProviderCapability::Monitorable,
    };
    status
}

/// 设置测试 locale 为英语，并在测试期间独占 locale 全局状态
pub fn setup_test_locale() -> crate::i18n::TestLocaleGuard {
    crate::i18n::test_locale_guard("en")
}
