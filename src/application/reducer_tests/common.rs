use crate::application::{AppEffect, AppSession, ContextEffect};
use crate::models::test_helpers::make_test_provider;
use crate::models::{
    AppSettings, ConnectionStatus, ProviderId, ProviderKind, SettingsCapability,
    TokenInputCapability,
};

pub(super) fn pid(kind: ProviderKind) -> ProviderId {
    ProviderId::BuiltIn(kind)
}

pub(super) fn make_session() -> AppSession {
    let providers = ProviderKind::all()
        .iter()
        .map(|k| make_test_provider(*k, ConnectionStatus::Disconnected))
        .collect();
    AppSession::new(AppSettings::default(), providers)
}

/// 构建一个不包含指定 provider 的 session
pub(super) fn make_session_without(excluded: ProviderKind) -> AppSession {
    let providers = ProviderKind::all()
        .iter()
        .filter(|k| **k != excluded)
        .map(|k| make_test_provider(*k, ConnectionStatus::Disconnected))
        .collect();
    AppSession::new(AppSettings::default(), providers)
}

pub(super) fn make_custom_token_provider(
    id: &str,
    credential_key: &'static str,
) -> crate::models::ProviderStatus {
    let provider_id = ProviderId::Custom(id.to_string());
    let mut metadata = crate::models::test_helpers::make_test_metadata(ProviderKind::Custom);
    metadata.display_name = "Custom Token".to_string();
    let mut status = crate::models::ProviderStatus::new(provider_id, metadata);
    status.settings_capability = SettingsCapability::TokenInput(TokenInputCapability {
        credential_key,
        placeholder_i18n_key: "copilot.token_placeholder",
        help_tip_i18n_key: "copilot.token_sources_tip",
        title_i18n_key: "copilot.github_login",
        description_i18n_key: "copilot.requires_auth",
        create_url: "https://example.com/token",
    });
    status
}

pub(super) fn make_custom_provider_status(id: &str) -> crate::models::ProviderStatus {
    let metadata = crate::models::test_helpers::make_test_metadata(ProviderKind::Custom);
    crate::models::ProviderStatus::new(ProviderId::Custom(id.to_string()), metadata)
}

pub(super) fn has_effect(effects: &[AppEffect], f: impl Fn(&AppEffect) -> bool) -> bool {
    effects.iter().any(f)
}

pub(super) fn has_render(effects: &[AppEffect]) -> bool {
    has_effect(effects, |e| {
        matches!(e, AppEffect::Context(ContextEffect::Render))
    })
}
