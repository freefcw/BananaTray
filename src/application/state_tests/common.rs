use super::super::*;
use crate::models::test_helpers::make_test_provider;
use crate::models::{ConnectionStatus, ProviderId, ProviderKind, SettingsCapability};

/// 快捷构造 ProviderId::BuiltIn
pub(super) fn pid(kind: ProviderKind) -> ProviderId {
    ProviderId::BuiltIn(kind)
}

pub(super) fn make_provider(kind: ProviderKind) -> ProviderStatus {
    make_test_provider(kind, ConnectionStatus::Disconnected)
}

pub(super) fn make_store(kinds: &[ProviderKind]) -> ProviderStore {
    ProviderStore {
        providers: kinds.iter().map(|k| make_provider(*k)).collect(),
    }
}

pub(super) fn make_settings(enabled: &[ProviderKind]) -> AppSettings {
    let mut s = AppSettings::default();
    for k in enabled {
        s.provider.set_provider_enabled(*k, true);
    }
    s
}

pub(super) fn make_nav(kind: ProviderKind) -> NavigationState {
    NavigationState {
        active_tab: crate::models::NavTab::Provider(pid(kind)),
        last_provider_id: pid(kind),
        prev_active_tab: None,
        generation: 0,
    }
}

pub(super) fn make_custom_status(id: &str) -> ProviderStatus {
    let provider_id = ProviderId::Custom(id.to_string());
    let metadata = crate::models::test_helpers::make_test_metadata(ProviderKind::Custom);
    ProviderStatus::new(provider_id, metadata)
}

pub(super) fn make_custom_status_with_name(id: &str, display_name: &str) -> ProviderStatus {
    let provider_id = ProviderId::Custom(id.to_string());
    let mut metadata = crate::models::test_helpers::make_test_metadata(ProviderKind::Custom);
    metadata.display_name = display_name.to_string();
    ProviderStatus::new(provider_id, metadata)
}

pub(super) fn make_custom_status_with_capability(
    id: &str,
    capability: SettingsCapability,
) -> ProviderStatus {
    let mut status = make_custom_status(id);
    status.settings_capability = capability;
    status
}
