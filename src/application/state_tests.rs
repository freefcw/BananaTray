use super::*;
use crate::models::test_helpers::make_test_provider;
use crate::models::{
    ConnectionStatus, DisplaySettings, ProviderId, ProviderKind, SettingsCapability,
    TokenInputCapability,
};

/// 快捷构造 ProviderId::BuiltIn
fn pid(kind: ProviderKind) -> ProviderId {
    ProviderId::BuiltIn(kind)
}

fn make_provider(kind: ProviderKind) -> ProviderStatus {
    make_test_provider(kind, ConnectionStatus::Disconnected)
}

fn make_store(kinds: &[ProviderKind]) -> ProviderStore {
    ProviderStore {
        providers: kinds.iter().map(|k| make_provider(*k)).collect(),
    }
}

fn make_settings(enabled: &[ProviderKind]) -> AppSettings {
    let mut s = AppSettings::default();
    for k in enabled {
        s.provider.set_provider_enabled(*k, true);
    }
    s
}

// ── ProviderStore ──────────────────────────────────────────

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

// ── NavigationState ────────────────────────────────────────

#[test]
fn nav_switch_to_provider() {
    let mut nav = NavigationState {
        active_tab: NavTab::Settings,
        last_provider_id: pid(ProviderKind::Claude),
        prev_active_tab: None,
        generation: 0,
    };
    nav.switch_to(NavTab::Provider(pid(ProviderKind::Gemini)));
    assert_eq!(nav.active_tab, NavTab::Provider(pid(ProviderKind::Gemini)));
    assert_eq!(nav.last_provider_id, pid(ProviderKind::Gemini));
}

#[test]
fn nav_switch_to_settings_preserves_last_provider() {
    let mut nav = NavigationState {
        active_tab: NavTab::Provider(pid(ProviderKind::Claude)),
        last_provider_id: pid(ProviderKind::Claude),
        prev_active_tab: None,
        generation: 0,
    };
    nav.switch_to(NavTab::Settings);
    assert_eq!(nav.active_tab, NavTab::Settings);
    assert_eq!(nav.last_provider_id, pid(ProviderKind::Claude));
}

#[test]
fn nav_switch_between_providers() {
    let mut nav = NavigationState {
        active_tab: NavTab::Provider(pid(ProviderKind::Claude)),
        last_provider_id: pid(ProviderKind::Claude),
        prev_active_tab: None,
        generation: 0,
    };
    nav.switch_to(NavTab::Provider(pid(ProviderKind::Gemini)));
    assert_eq!(nav.active_tab, NavTab::Provider(pid(ProviderKind::Gemini)));
    assert_eq!(nav.last_provider_id, pid(ProviderKind::Gemini));

    nav.switch_to(NavTab::Provider(pid(ProviderKind::Copilot)));
    assert_eq!(nav.last_provider_id, pid(ProviderKind::Copilot));
}

#[test]
fn nav_fallback_when_current_disabled() {
    let store = make_store(&[ProviderKind::Claude, ProviderKind::Gemini]);
    let mut nav = NavigationState {
        active_tab: NavTab::Provider(pid(ProviderKind::Claude)),
        last_provider_id: pid(ProviderKind::Claude),
        prev_active_tab: None,
        generation: 0,
    };
    let settings = make_settings(&[ProviderKind::Claude, ProviderKind::Gemini]);
    nav.fallback_on_disable(&pid(ProviderKind::Claude), &store.providers, &settings);
    assert_eq!(nav.active_tab, NavTab::Provider(pid(ProviderKind::Gemini)));
    assert_eq!(nav.last_provider_id, pid(ProviderKind::Gemini));
}

#[test]
fn nav_fallback_noop_when_not_current() {
    let store = make_store(&[ProviderKind::Gemini]);
    let mut nav = NavigationState {
        active_tab: NavTab::Provider(pid(ProviderKind::Gemini)),
        last_provider_id: pid(ProviderKind::Gemini),
        prev_active_tab: None,
        generation: 0,
    };
    let settings = make_settings(&[ProviderKind::Gemini]);
    nav.fallback_on_disable(&pid(ProviderKind::Claude), &store.providers, &settings);
    assert_eq!(nav.active_tab, NavTab::Provider(pid(ProviderKind::Gemini)));
    assert_eq!(nav.last_provider_id, pid(ProviderKind::Gemini));
}

#[test]
fn nav_fallback_noop_when_on_settings_tab() {
    let store = make_store(&[ProviderKind::Gemini]);
    let mut nav = NavigationState {
        active_tab: NavTab::Settings,
        last_provider_id: pid(ProviderKind::Claude),
        prev_active_tab: None,
        generation: 0,
    };
    let settings = make_settings(&[ProviderKind::Gemini]);
    nav.fallback_on_disable(&pid(ProviderKind::Claude), &store.providers, &settings);
    assert_eq!(nav.active_tab, NavTab::Settings);
}

#[test]
fn nav_fallback_no_other_enabled_stays_put() {
    let store = make_store(&[ProviderKind::Claude]);
    let mut nav = NavigationState {
        active_tab: NavTab::Provider(pid(ProviderKind::Claude)),
        last_provider_id: pid(ProviderKind::Claude),
        prev_active_tab: None,
        generation: 0,
    };
    let settings = make_settings(&[ProviderKind::Claude]);
    nav.fallback_on_disable(&pid(ProviderKind::Claude), &store.providers, &settings);
    assert_eq!(nav.active_tab, NavTab::Provider(pid(ProviderKind::Claude)));
    assert_eq!(nav.last_provider_id, pid(ProviderKind::Claude));
}

#[test]
fn nav_fallback_picks_first_enabled_in_order() {
    let store = make_store(&[
        ProviderKind::Claude,
        ProviderKind::Gemini,
        ProviderKind::Copilot,
    ]);
    let mut nav = NavigationState {
        active_tab: NavTab::Provider(pid(ProviderKind::Claude)),
        last_provider_id: pid(ProviderKind::Claude),
        prev_active_tab: None,
        generation: 0,
    };
    let settings = make_settings(&[
        ProviderKind::Claude,
        ProviderKind::Gemini,
        ProviderKind::Copilot,
    ]);
    nav.fallback_on_disable(&pid(ProviderKind::Claude), &store.providers, &settings);
    assert_eq!(nav.active_tab, NavTab::Provider(pid(ProviderKind::Gemini)));
}

// ── SettingsUiState ────────────────────────────────────────

#[test]
fn settings_ui_default_values() {
    let ui = SettingsUiState {
        active_tab: SettingsTab::General,
        selected_provider: pid(ProviderKind::Claude),
        cadence_dropdown_open: false,
        token_editing_provider: None,
        adding_newapi: false,
        editing_newapi: None,
        adding_provider: false,
        confirming_remove_provider: false,
        confirming_delete_newapi: false,
    };
    assert_eq!(ui.active_tab, SettingsTab::General);
    assert!(!ui.cadence_dropdown_open);
}

#[test]
fn debug_ui_default_values() {
    let debug = DebugUiState::default();
    assert!(debug.selected_provider.is_none());
    assert!(!debug.refresh_active);
    assert!(debug.prev_log_level.is_none());
}

// ── HeaderStatusText ────────────────────────────────────────

#[test]
fn header_status_missing_provider() {
    let store = make_store(&[]);
    let nav = NavigationState {
        active_tab: NavTab::Provider(pid(ProviderKind::Claude)),
        last_provider_id: pid(ProviderKind::Claude),
        prev_active_tab: None,
        generation: 0,
    };
    let (kind, elapsed) = compute_header_status(&nav, &store);
    assert_eq!(kind, HeaderStatusKind::Offline);
    assert!(elapsed.is_none());
}

#[test]
fn header_status_refreshing() {
    let mut store = make_store(&[ProviderKind::Claude]);
    let p = store.find_by_id_mut(&pid(ProviderKind::Claude)).unwrap();
    p.connection = ConnectionStatus::Refreshing;

    let nav = NavigationState {
        active_tab: NavTab::Provider(pid(ProviderKind::Claude)),
        last_provider_id: pid(ProviderKind::Claude),
        prev_active_tab: None,
        generation: 0,
    };
    let (kind, elapsed) = compute_header_status(&nav, &store);
    assert_eq!(kind, HeaderStatusKind::Syncing);
    assert!(elapsed.is_none());
}

#[test]
fn header_status_disconnected() {
    let mut store = make_store(&[ProviderKind::Claude]);
    let p = store.find_by_id_mut(&pid(ProviderKind::Claude)).unwrap();
    p.connection = ConnectionStatus::Disconnected;

    let nav = NavigationState {
        active_tab: NavTab::Provider(pid(ProviderKind::Claude)),
        last_provider_id: pid(ProviderKind::Claude),
        prev_active_tab: None,
        generation: 0,
    };
    let (kind, elapsed) = compute_header_status(&nav, &store);
    assert_eq!(kind, HeaderStatusKind::Offline);
    assert!(elapsed.is_none());
}

#[test]
fn header_status_synced_now() {
    let mut store = make_store(&[ProviderKind::Claude]);
    let p = store.find_by_id_mut(&pid(ProviderKind::Claude)).unwrap();
    p.connection = ConnectionStatus::Connected;
    p.last_refreshed_instant = Some(std::time::Instant::now());

    let nav = NavigationState {
        active_tab: NavTab::Provider(pid(ProviderKind::Claude)),
        last_provider_id: pid(ProviderKind::Claude),
        prev_active_tab: None,
        generation: 0,
    };
    let (kind, elapsed) = compute_header_status(&nav, &store);
    assert_eq!(kind, HeaderStatusKind::Synced);
    assert!(elapsed.unwrap() < 60);
}

#[test]
fn header_status_synced_minutes_ago() {
    let mut store = make_store(&[ProviderKind::Claude]);
    let p = store.find_by_id_mut(&pid(ProviderKind::Claude)).unwrap();
    p.connection = ConnectionStatus::Connected;
    p.last_refreshed_instant =
        Some(std::time::Instant::now() - std::time::Duration::from_secs(300));

    let nav = NavigationState {
        active_tab: NavTab::Provider(pid(ProviderKind::Claude)),
        last_provider_id: pid(ProviderKind::Claude),
        prev_active_tab: None,
        generation: 0,
    };
    let (kind, elapsed) = compute_header_status(&nav, &store);
    assert_eq!(kind, HeaderStatusKind::Stale);
    assert!(elapsed.unwrap() >= 300);
}

#[test]
fn header_status_synced_hours_ago() {
    let mut store = make_store(&[ProviderKind::Claude]);
    let p = store.find_by_id_mut(&pid(ProviderKind::Claude)).unwrap();
    p.connection = ConnectionStatus::Connected;
    p.last_refreshed_instant =
        Some(std::time::Instant::now() - std::time::Duration::from_secs(7200));

    let nav = NavigationState {
        active_tab: NavTab::Provider(pid(ProviderKind::Claude)),
        last_provider_id: pid(ProviderKind::Claude),
        prev_active_tab: None,
        generation: 0,
    };
    let (kind, elapsed) = compute_header_status(&nav, &store);
    assert_eq!(kind, HeaderStatusKind::Stale);
    assert!(elapsed.unwrap() >= 7200);
}

// ── provider_panel_flags ──────────────────────────────────

#[test]
fn panel_flags_account_visible_hides_dashboard_row() {
    let settings = AppSettings {
        display: DisplaySettings {
            show_account_info: true,
            show_dashboard_button: true,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut provider = make_test_provider(ProviderKind::Gemini, ConnectionStatus::Connected);
    provider.account_email = Some("user@example.com".to_string());

    let flags = provider_panel_flags(&settings, &provider);
    assert!(flags.show_account_info);
    assert!(!flags.show_dashboard_row);
    assert!(flags.has_dashboard_url);
}

#[test]
fn panel_flags_no_email_shows_dashboard_row() {
    let settings = AppSettings {
        display: DisplaySettings {
            show_account_info: true,
            show_dashboard_button: true,
            ..Default::default()
        },
        ..Default::default()
    };

    let provider = make_test_provider(ProviderKind::Gemini, ConnectionStatus::Connected);
    // account_email is None by default

    let flags = provider_panel_flags(&settings, &provider);
    assert!(!flags.show_account_info);
    assert!(flags.show_dashboard_row);
}

#[test]
fn panel_flags_setting_off_shows_dashboard_row() {
    let settings = AppSettings {
        display: DisplaySettings {
            show_account_info: false,
            show_dashboard_button: true,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut provider = make_test_provider(ProviderKind::Gemini, ConnectionStatus::Connected);
    provider.account_email = Some("user@example.com".to_string());

    let flags = provider_panel_flags(&settings, &provider);
    assert!(!flags.show_account_info);
    assert!(flags.show_dashboard_row);
}

#[test]
fn panel_flags_dashboard_setting_off() {
    let settings = AppSettings {
        display: DisplaySettings {
            show_account_info: true,
            show_dashboard_button: false,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut provider = make_test_provider(ProviderKind::Gemini, ConnectionStatus::Connected);
    provider.account_email = Some("user@example.com".to_string());

    let flags = provider_panel_flags(&settings, &provider);
    assert!(flags.show_account_info);
    assert!(!flags.show_dashboard_row);
    // dashboard_url 仍然存在（账户卡片 chevron 可用）
    assert!(flags.has_dashboard_url);
}

// ── HeaderStatusText ────────────────────────────────────────

#[test]
fn header_status_error() {
    let mut store = make_store(&[ProviderKind::Claude]);
    let p = store.find_by_id_mut(&pid(ProviderKind::Claude)).unwrap();
    p.connection = ConnectionStatus::Error;
    // 注意：如果是 Error 状态且 last_refreshed_instant 不为 None，
    // 我们会显示最后刷新时间（在前面分支处理了），所以这里设为 None 以测试 Error 分支
    p.last_refreshed_instant = None;

    let nav = NavigationState {
        active_tab: NavTab::Provider(pid(ProviderKind::Claude)),
        last_provider_id: pid(ProviderKind::Claude),
        prev_active_tab: None,
        generation: 0,
    };
    let (kind, elapsed) = compute_header_status(&nav, &store);
    assert_eq!(kind, HeaderStatusKind::Offline);
    assert!(elapsed.is_none());
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

// ── sync_custom_providers (热重载) ────────────────────────

fn make_custom_status(id: &str) -> ProviderStatus {
    let provider_id = ProviderId::Custom(id.to_string());
    let metadata = crate::models::test_helpers::make_test_metadata(ProviderKind::Custom);
    ProviderStatus::new(provider_id, metadata)
}

fn make_custom_status_with_name(id: &str, display_name: &str) -> ProviderStatus {
    let provider_id = ProviderId::Custom(id.to_string());
    let mut metadata = crate::models::test_helpers::make_test_metadata(ProviderKind::Custom);
    metadata.display_name = display_name.to_string();
    ProviderStatus::new(provider_id, metadata)
}

fn make_custom_status_with_capability(id: &str, capability: SettingsCapability) -> ProviderStatus {
    let mut status = make_custom_status(id);
    status.settings_capability = capability;
    status
}

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

// ── compute_popup_height ─────────────────────────────────

fn make_nav(kind: ProviderKind) -> NavigationState {
    NavigationState {
        active_tab: NavTab::Provider(pid(kind)),
        last_provider_id: pid(kind),
        prev_active_tab: None,
        generation: 0,
    }
}

#[test]
fn popup_height_missing_provider_returns_min() {
    let store = make_store(&[]);
    let nav = make_nav(ProviderKind::Claude);
    let settings = AppSettings::default();

    let h = compute_popup_height(&nav, &store, &settings);
    assert_eq!(h, crate::models::PopupLayout::MIN_HEIGHT);
}

#[test]
fn popup_height_empty_quotas_with_dashboard() {
    // make_test_provider 有 dashboard_url，无 account_email → show_dashboard = true
    let store = make_store(&[ProviderKind::Claude]);
    let nav = make_nav(ProviderKind::Claude);
    let settings = AppSettings::default();

    let h = compute_popup_height(&nav, &store, &settings);
    let expected = crate::models::compute_popup_height_detailed(1, true, false);
    assert_eq!(h, expected);
}

#[test]
fn popup_height_uses_last_provider_on_settings_tab() {
    let store = make_store(&[ProviderKind::Claude]);
    let nav = NavigationState {
        active_tab: NavTab::Settings,
        last_provider_id: pid(ProviderKind::Claude),
        prev_active_tab: None,
        generation: 0,
    };
    let settings = AppSettings::default();

    let h = compute_popup_height(&nav, &store, &settings);
    let expected = crate::models::compute_popup_height_detailed(1, true, false);
    assert_eq!(h, expected);
}

#[test]
fn popup_height_with_visible_quotas() {
    use crate::models::QuotaInfo;

    let mut store = make_store(&[ProviderKind::Claude]);
    let p = store.find_by_id_mut(&pid(ProviderKind::Claude)).unwrap();
    p.quotas = vec![
        QuotaInfo::new("Session", 50.0, 100.0),
        QuotaInfo::new("Weekly", 20.0, 100.0),
    ];

    let nav = make_nav(ProviderKind::Claude);
    let settings = AppSettings::default();

    let h = compute_popup_height(&nav, &store, &settings);
    let expected = crate::models::compute_popup_height_detailed(2, true, false);
    assert_eq!(h, expected);
}

#[test]
fn popup_height_all_quotas_hidden_shows_one_card() {
    use crate::models::{QuotaInfo, QuotaType};

    let mut store = make_store(&[ProviderKind::Claude]);
    let p = store.find_by_id_mut(&pid(ProviderKind::Claude)).unwrap();
    p.quotas = vec![QuotaInfo::with_details(
        "Session",
        50.0,
        100.0,
        QuotaType::Session,
        None,
    )];

    let nav = make_nav(ProviderKind::Claude);
    let mut settings = AppSettings::default();
    settings
        .provider
        .toggle_quota_visibility(ProviderKind::Claude, "session".to_string());

    let h = compute_popup_height(&nav, &store, &settings);
    // 全部隐藏时至少预留 1 个卡片高度，dashboard 仍可见
    let expected = crate::models::compute_popup_height_detailed(1, true, false);
    assert_eq!(h, expected);
}

#[test]
fn popup_height_account_info_hides_dashboard_row() {
    // 有 account_email 时 show_account_info=true，dashboard_row 互斥隐藏
    let mut store = make_store(&[ProviderKind::Claude]);
    let p = store.find_by_id_mut(&pid(ProviderKind::Claude)).unwrap();
    p.account_email = Some("user@example.com".to_string());

    let nav = make_nav(ProviderKind::Claude);
    let settings = AppSettings::default();

    let h = compute_popup_height(&nav, &store, &settings);
    // account_info 可见时 dashboard_row 被互斥隐藏
    let expected = crate::models::compute_popup_height_detailed(1, false, true);
    assert_eq!(h, expected);
}

// ── current_provider_status ────────────────────────────────────

#[test]
fn current_provider_status_disconnected_returns_green() {
    // 当前 Provider 未连接 → 返回 Green（安全默认值）
    let store = make_store(&[ProviderKind::Claude]);
    let settings = make_settings(&[ProviderKind::Claude]);
    let session = AppSession::new(settings, store.providers);
    assert_eq!(session.current_provider_status(), StatusLevel::Green);
}

#[test]
fn current_provider_status_connected_green() {
    use crate::models::QuotaInfo;
    let mut store = make_store(&[ProviderKind::Claude]);
    let p = store.find_by_id_mut(&pid(ProviderKind::Claude)).unwrap();
    p.connection = ConnectionStatus::Connected;
    p.quotas = vec![QuotaInfo::new("session", 10.0, 100.0)]; // 90% remaining → Green

    let settings = make_settings(&[ProviderKind::Claude]);
    let session = AppSession::new(settings, store.providers);
    assert_eq!(session.current_provider_status(), StatusLevel::Green);
}

#[test]
fn current_provider_status_connected_red() {
    use crate::models::QuotaInfo;
    let mut store = make_store(&[ProviderKind::Claude]);
    let p = store.find_by_id_mut(&pid(ProviderKind::Claude)).unwrap();
    p.connection = ConnectionStatus::Connected;
    p.quotas = vec![QuotaInfo::new("session", 95.0, 100.0)]; // 5% remaining → Red

    let settings = make_settings(&[ProviderKind::Claude]);
    let session = AppSession::new(settings, store.providers);
    assert_eq!(session.current_provider_status(), StatusLevel::Red);
}

#[test]
fn current_provider_status_ignores_other_providers() {
    use crate::models::QuotaInfo;
    let mut store = make_store(&[ProviderKind::Claude, ProviderKind::Gemini]);

    // Claude（当前 Provider）：Green
    let p1 = store.find_by_id_mut(&pid(ProviderKind::Claude)).unwrap();
    p1.connection = ConnectionStatus::Connected;
    p1.quotas = vec![QuotaInfo::new("session", 10.0, 100.0)];

    // Gemini：Red — 但不影响图标
    let p2 = store.find_by_id_mut(&pid(ProviderKind::Gemini)).unwrap();
    p2.connection = ConnectionStatus::Connected;
    p2.quotas = vec![QuotaInfo::new("session", 95.0, 100.0)];

    let settings = make_settings(&[ProviderKind::Claude, ProviderKind::Gemini]);
    let session = AppSession::new(settings, store.providers);
    // 当前 Provider = Claude (Green)，Gemini (Red) 不影响
    assert_eq!(session.current_provider_status(), StatusLevel::Green);
}

#[test]
fn current_provider_status_follows_last_provider_id() {
    use crate::models::QuotaInfo;
    let mut store = make_store(&[ProviderKind::Claude, ProviderKind::Gemini]);

    let p1 = store.find_by_id_mut(&pid(ProviderKind::Claude)).unwrap();
    p1.connection = ConnectionStatus::Connected;
    p1.quotas = vec![QuotaInfo::new("session", 10.0, 100.0)]; // Green

    let p2 = store.find_by_id_mut(&pid(ProviderKind::Gemini)).unwrap();
    p2.connection = ConnectionStatus::Connected;
    p2.quotas = vec![QuotaInfo::new("session", 95.0, 100.0)]; // Red

    let settings = make_settings(&[ProviderKind::Claude, ProviderKind::Gemini]);
    let mut session = AppSession::new(settings, store.providers);

    // 切换到 Gemini
    session.nav.last_provider_id = pid(ProviderKind::Gemini);
    assert_eq!(session.current_provider_status(), StatusLevel::Red);

    // 切回 Claude
    session.nav.last_provider_id = pid(ProviderKind::Claude);
    assert_eq!(session.current_provider_status(), StatusLevel::Green);
}

// ── AppSession::new 自动注册 ──────────────────────────────

#[test]
fn session_new_auto_registers_unregistered_custom_provider() {
    let mut store = make_store(&[ProviderKind::Claude]);
    let custom_id = ProviderId::Custom("my-relay:newapi".to_string());
    let metadata = crate::models::test_helpers::make_test_metadata(ProviderKind::Custom);
    store
        .providers
        .push(ProviderStatus::new(custom_id.clone(), metadata));

    // settings 中没有 custom provider 的任何条目
    let settings = make_settings(&[ProviderKind::Claude]);
    assert!(!settings
        .provider
        .enabled_providers
        .contains_key("my-relay:newapi"));

    let session = AppSession::new(settings, store.providers);

    // 自动启用
    assert!(session.settings.provider.is_enabled(&custom_id));
    // 自动加入 sidebar
    assert!(session
        .settings
        .provider
        .sidebar_providers
        .contains(&"my-relay:newapi".to_string()));
}

#[test]
fn session_new_preserves_existing_custom_provider_state() {
    let mut store = make_store(&[ProviderKind::Claude]);
    let custom_id = ProviderId::Custom("my-relay:newapi".to_string());
    let metadata = crate::models::test_helpers::make_test_metadata(ProviderKind::Custom);
    store
        .providers
        .push(ProviderStatus::new(custom_id.clone(), metadata));

    // 已手动禁用的 custom provider 不应被重新启用
    let mut settings = make_settings(&[ProviderKind::Claude]);
    settings.provider.set_enabled(&custom_id, false);

    let session = AppSession::new(settings, store.providers);

    // 保持禁用状态（用户显式关闭的不覆盖）
    assert!(!session.settings.provider.is_enabled(&custom_id));
}
