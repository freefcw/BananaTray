use super::super::*;
use super::common::*;
use crate::models::{
    ConnectionStatus, ProviderId, ProviderKind, SettingsCapability, TokenInputCapability,
};

#[test]
fn sync_adds_new_custom_provider() {
    let mut store = make_store(&[ProviderKind::Claude]);
    let new_statuses = vec![
        make_provider(ProviderKind::Claude),
        make_custom_status("myapi:cli"),
    ];

    let affected = store.sync_custom_providers(&new_statuses);

    assert_eq!(store.providers.len(), 2);
    assert!(store
        .find_by_id(&ProviderId::Custom("myapi:cli".to_string()))
        .is_some());
    assert!(affected.contains(&ProviderId::Custom("myapi:cli".to_string())));
}

#[test]
fn sync_removes_deleted_custom_but_keeps_builtin() {
    let mut store = make_store(&[ProviderKind::Claude]);
    let custom_id = ProviderId::Custom("old:cli".to_string());
    store.providers.push(make_custom_status("old:cli"));
    assert_eq!(store.providers.len(), 2);

    let new_statuses = vec![make_provider(ProviderKind::Claude)];
    let affected = store.sync_custom_providers(&new_statuses);

    assert_eq!(store.providers.len(), 1);
    assert!(store.find_by_id(&custom_id).is_none());
    assert!(store.find_by_id(&pid(ProviderKind::Claude)).is_some());
    assert!(affected.is_empty());
}

#[test]
fn sync_updates_metadata_for_changed_custom() {
    let mut store = make_store(&[ProviderKind::Claude]);
    store
        .providers
        .push(make_custom_status_with_name("myapi:cli", "Old Name"));

    let new_statuses = vec![
        make_provider(ProviderKind::Claude),
        make_custom_status_with_name("myapi:cli", "New Name"),
    ];
    let affected = store.sync_custom_providers(&new_statuses);

    let updated = store
        .find_by_id(&ProviderId::Custom("myapi:cli".to_string()))
        .unwrap();
    assert_eq!(updated.metadata.display_name, "New Name");
    assert!(affected.contains(&ProviderId::Custom("myapi:cli".to_string())));
}

#[test]
fn sync_does_not_mark_unchanged_custom_as_affected() {
    let mut store = make_store(&[ProviderKind::Claude]);
    store.providers.push(make_custom_status("myapi:cli"));

    let new_statuses = vec![
        make_provider(ProviderKind::Claude),
        make_custom_status("myapi:cli"),
    ];
    let affected = store.sync_custom_providers(&new_statuses);

    assert!(affected.is_empty());
}

#[test]
fn sync_preserves_runtime_state_for_existing_custom() {
    let mut store = make_store(&[ProviderKind::Claude]);
    let mut custom = make_custom_status("myapi:cli");
    custom.connection = ConnectionStatus::Connected;
    custom.account_email = Some("user@example.com".to_string());
    store.providers.push(custom);

    let new_statuses = vec![
        make_provider(ProviderKind::Claude),
        make_custom_status_with_name("myapi:cli", "Updated Name"),
    ];
    store.sync_custom_providers(&new_statuses);

    let p = store
        .find_by_id(&ProviderId::Custom("myapi:cli".to_string()))
        .unwrap();
    assert_eq!(p.metadata.display_name, "Updated Name");
    assert_eq!(p.connection, ConnectionStatus::Connected);
    assert_eq!(p.account_email.as_deref(), Some("user@example.com"));
}

#[test]
fn sync_updates_settings_capability_for_changed_custom() {
    let mut store = make_store(&[ProviderKind::Claude]);
    store.providers.push(make_custom_status("myapi:cli"));

    let capability = SettingsCapability::TokenInput(TokenInputCapability {
        credential_key: "custom_token",
        placeholder_i18n_key: "copilot.token_placeholder",
        help_tip_i18n_key: "copilot.token_sources_tip",
        title_i18n_key: "copilot.github_login",
        description_i18n_key: "copilot.requires_auth",
        create_url: "https://example.com/token",
    });
    let new_statuses = vec![
        make_provider(ProviderKind::Claude),
        make_custom_status_with_capability("myapi:cli", capability.clone()),
    ];

    let affected = store.sync_custom_providers(&new_statuses);

    let updated = store
        .find_by_id(&ProviderId::Custom("myapi:cli".to_string()))
        .unwrap();
    assert_eq!(updated.settings_capability, capability);
    assert!(affected.contains(&ProviderId::Custom("myapi:cli".to_string())));
}

#[test]
fn sync_removes_all_custom_when_new_list_has_none() {
    let mut store = make_store(&[ProviderKind::Claude]);
    store.providers.push(make_custom_status("a:cli"));
    store.providers.push(make_custom_status("b:cli"));
    assert_eq!(store.providers.len(), 3);

    let new_statuses = vec![make_provider(ProviderKind::Claude)];
    store.sync_custom_providers(&new_statuses);

    assert_eq!(store.providers.len(), 1);
    assert!(store.custom_provider_ids().is_empty());
}
