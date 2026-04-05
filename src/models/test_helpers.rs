//! 共享测试工具函数
//!
//! 统一 ProviderStatus / ProviderMetadata 的构造逻辑，
//! 消除 quota.rs / app_state.rs / provider_logic.rs / selectors.rs 中的重复定义。

use super::provider::{ProviderId, ProviderKind, ProviderMetadata};
use super::quota::{ConnectionStatus, ErrorKind, ProviderStatus};

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
pub fn make_test_provider(kind: ProviderKind, connection: ConnectionStatus) -> ProviderStatus {
    ProviderStatus {
        provider_id: ProviderId::BuiltIn(kind),
        metadata: make_test_metadata(kind),
        enabled: true,
        connection,
        quotas: vec![],
        account_email: None,
        is_paid: false,
        account_tier: None,
        last_updated_at: None,
        error_message: None,
        error_kind: ErrorKind::default(),
        last_refreshed_instant: None,
    }
}

/// 设置测试 locale 为英语，并在测试期间独占 locale 全局状态
pub fn setup_test_locale() -> crate::i18n::TestLocaleGuard {
    crate::i18n::test_locale_guard("en")
}
