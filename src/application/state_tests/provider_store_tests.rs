use super::super::*;
use super::common::*;
use crate::models::{ConnectionStatus, ProviderId, ProviderKind};

// ── ProviderStore 基本操作 ──────────────────────────────────

#[test]
fn store_find_existing() {
    let store = make_store(&[ProviderKind::Claude, ProviderKind::Gemini]);
    assert!(store.find_by_id(&pid(ProviderKind::Claude)).is_some());
    assert!(store.find_by_id(&pid(ProviderKind::Gemini)).is_some());
}

#[test]
fn store_find_missing() {
    let store = make_store(&[ProviderKind::Claude]);
    assert!(store.find_by_id(&pid(ProviderKind::Copilot)).is_none());
}

#[test]
fn store_find_returns_correct_provider() {
    let store = make_store(&[
        ProviderKind::Claude,
        ProviderKind::Gemini,
        ProviderKind::Copilot,
    ]);
    let p = store.find_by_id(&pid(ProviderKind::Gemini)).unwrap();
    assert_eq!(p.kind(), ProviderKind::Gemini);
}

#[test]
fn store_find_mut_modifies_connection() {
    let mut store = make_store(&[ProviderKind::Claude]);
    store
        .find_by_id_mut(&pid(ProviderKind::Claude))
        .unwrap()
        .connection = ConnectionStatus::Error;
    assert_eq!(
        store
            .find_by_id(&pid(ProviderKind::Claude))
            .unwrap()
            .connection,
        ConnectionStatus::Error
    );
}

#[test]
fn store_find_mut_missing_returns_none() {
    let mut store = make_store(&[ProviderKind::Claude]);
    assert!(store.find_by_id_mut(&pid(ProviderKind::Copilot)).is_none());
}

#[test]
fn store_mark_refreshing() {
    let mut store = make_store(&[ProviderKind::Claude]);
    assert_eq!(
        store
            .find_by_id(&pid(ProviderKind::Claude))
            .unwrap()
            .connection,
        ConnectionStatus::Disconnected
    );
    store.mark_refreshing_by_id(&pid(ProviderKind::Claude));
    assert_eq!(
        store
            .find_by_id(&pid(ProviderKind::Claude))
            .unwrap()
            .connection,
        ConnectionStatus::Refreshing
    );
}

#[test]
fn store_mark_refreshing_missing_is_noop() {
    let mut store = make_store(&[ProviderKind::Claude]);
    // Should not panic
    store.mark_refreshing_by_id(&pid(ProviderKind::Copilot));
}

// ── ProviderStore: find_by_id / custom_provider_ids ──────

#[test]
fn store_find_by_id_builtin() {
    let store = make_store(&[ProviderKind::Claude]);
    assert!(store.find_by_id(&pid(ProviderKind::Claude)).is_some());
    assert!(store.find_by_id(&pid(ProviderKind::Gemini)).is_none());
}

#[test]
fn store_find_by_id_custom() {
    let custom_id = ProviderId::Custom("myai:cli".to_string());
    let mut store = make_store(&[]);
    let metadata = crate::models::test_helpers::make_test_metadata(ProviderKind::Custom);
    store
        .providers
        .push(ProviderStatus::new(custom_id.clone(), metadata));

    assert!(store.find_by_id(&custom_id).is_some());
    assert!(store.find_by_id(&pid(ProviderKind::Claude)).is_none());
}

#[test]
fn store_find_by_id_mut_custom() {
    let custom_id = ProviderId::Custom("myai:cli".to_string());
    let mut store = make_store(&[]);
    let metadata = crate::models::test_helpers::make_test_metadata(ProviderKind::Custom);
    store
        .providers
        .push(ProviderStatus::new(custom_id.clone(), metadata));

    store.find_by_id_mut(&custom_id).unwrap().connection = ConnectionStatus::Error;
    assert_eq!(
        store.find_by_id(&custom_id).unwrap().connection,
        ConnectionStatus::Error
    );
}

#[test]
fn store_mark_refreshing_by_id_custom() {
    let custom_id = ProviderId::Custom("myai:cli".to_string());
    let mut store = make_store(&[]);
    let metadata = crate::models::test_helpers::make_test_metadata(ProviderKind::Custom);
    store
        .providers
        .push(ProviderStatus::new(custom_id.clone(), metadata));

    store.mark_refreshing_by_id(&custom_id);
    assert_eq!(
        store.find_by_id(&custom_id).unwrap().connection,
        ConnectionStatus::Refreshing
    );
}

#[test]
fn store_custom_provider_ids() {
    let custom1 = ProviderId::Custom("a:cli".to_string());
    let custom2 = ProviderId::Custom("b:cli".to_string());
    let metadata = crate::models::test_helpers::make_test_metadata(ProviderKind::Custom);
    let mut store = make_store(&[ProviderKind::Claude]);
    store
        .providers
        .push(ProviderStatus::new(custom1.clone(), metadata.clone()));
    store
        .providers
        .push(ProviderStatus::new(custom2.clone(), metadata));

    let ids = store.custom_provider_ids();
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&custom1));
    assert!(ids.contains(&custom2));
}

#[test]
fn store_custom_provider_ids_empty_when_no_custom() {
    let store = make_store(&[ProviderKind::Claude]);
    assert!(store.custom_provider_ids().is_empty());
}

// ── ProviderStore::refreshable_provider_ids() ─────────────

#[test]
fn refreshable_provider_ids_filters_enabled_monitorable() {
    let store = make_store(&[
        ProviderKind::Claude, // Monitorable
        ProviderKind::Gemini, // Monitorable
        ProviderKind::Kilo,   // Placeholder
    ]);
    let settings = make_settings(&[ProviderKind::Claude, ProviderKind::Kilo]);

    let ids = store.refreshable_provider_ids(&settings);

    // Claude: enabled + Monitorable → included
    assert!(ids.contains(&pid(ProviderKind::Claude)));
    // Gemini: not enabled → excluded
    assert!(!ids.contains(&pid(ProviderKind::Gemini)));
    // Kilo: enabled but Placeholder → excluded
    assert!(!ids.contains(&pid(ProviderKind::Kilo)));
    assert_eq!(ids.len(), 1);
}

#[test]
fn refreshable_provider_ids_empty_when_none_enabled() {
    let store = make_store(&[ProviderKind::Claude]);
    let settings = AppSettings::default(); // nothing enabled

    assert!(store.refreshable_provider_ids(&settings).is_empty());
}
