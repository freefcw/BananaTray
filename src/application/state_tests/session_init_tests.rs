use super::super::*;
use super::common::*;
use crate::models::{NavTab, ProviderId, ProviderKind, QuotaInfo, StatusLevel};

// ── worst_enabled_provider_status ──────────────────────────────

#[test]
fn worst_enabled_status_no_connected_returns_green() {
    // 无已连接 Provider → 返回 Green（安全默认值）
    let store = make_store(&[ProviderKind::Claude]);
    let settings = make_settings(&[ProviderKind::Claude]);
    let session = AppSession::new(settings, store.providers);
    assert_eq!(session.worst_enabled_provider_status(), StatusLevel::Green);
}

#[test]
fn worst_enabled_status_connected_green() {
    let mut store = make_store(&[ProviderKind::Claude]);
    let p = store.find_by_id_mut(&pid(ProviderKind::Claude)).unwrap();
    p.connection = crate::models::ConnectionStatus::Connected;
    p.quotas = vec![QuotaInfo::new("session", 10.0, 100.0)]; // 90% remaining → Green

    let settings = make_settings(&[ProviderKind::Claude]);
    let session = AppSession::new(settings, store.providers);
    assert_eq!(session.worst_enabled_provider_status(), StatusLevel::Green);
}

#[test]
fn worst_enabled_status_connected_red() {
    let mut store = make_store(&[ProviderKind::Claude]);
    let p = store.find_by_id_mut(&pid(ProviderKind::Claude)).unwrap();
    p.connection = crate::models::ConnectionStatus::Connected;
    p.quotas = vec![QuotaInfo::new("session", 95.0, 100.0)]; // 5% remaining → Red

    let settings = make_settings(&[ProviderKind::Claude]);
    let session = AppSession::new(settings, store.providers);
    assert_eq!(session.worst_enabled_provider_status(), StatusLevel::Red);
}

#[test]
fn worst_enabled_status_aggregates_all_enabled_providers() {
    let mut store = make_store(&[ProviderKind::Claude, ProviderKind::Gemini]);

    // Claude：Green
    let p1 = store.find_by_id_mut(&pid(ProviderKind::Claude)).unwrap();
    p1.connection = crate::models::ConnectionStatus::Connected;
    p1.quotas = vec![QuotaInfo::new("session", 10.0, 100.0)];

    // Gemini：Red — 未选中也会反映到综合状态
    let p2 = store.find_by_id_mut(&pid(ProviderKind::Gemini)).unwrap();
    p2.connection = crate::models::ConnectionStatus::Connected;
    p2.quotas = vec![QuotaInfo::new("session", 95.0, 100.0)];

    let settings = make_settings(&[ProviderKind::Claude, ProviderKind::Gemini]);
    let session = AppSession::new(settings, store.providers);
    // 聚合取最坏值：Gemini (Red) 决定图标颜色
    assert_eq!(session.worst_enabled_provider_status(), StatusLevel::Red);
}

#[test]
fn worst_enabled_status_ignores_disabled_providers() {
    let mut store = make_store(&[ProviderKind::Claude, ProviderKind::Gemini]);

    let p1 = store.find_by_id_mut(&pid(ProviderKind::Claude)).unwrap();
    p1.connection = crate::models::ConnectionStatus::Connected;
    p1.quotas = vec![QuotaInfo::new("session", 10.0, 100.0)]; // Green

    let p2 = store.find_by_id_mut(&pid(ProviderKind::Gemini)).unwrap();
    p2.connection = crate::models::ConnectionStatus::Connected;
    p2.quotas = vec![QuotaInfo::new("session", 95.0, 100.0)]; // Red

    // Gemini 被禁用 → 不参与综合状态
    let mut settings = make_settings(&[ProviderKind::Claude]);
    settings
        .provider
        .set_enabled(&pid(ProviderKind::Gemini), false);
    let session = AppSession::new(settings, store.providers);
    assert_eq!(session.worst_enabled_provider_status(), StatusLevel::Green);
}

#[test]
fn worst_enabled_status_ignores_disconnected_providers() {
    let mut store = make_store(&[ProviderKind::Claude, ProviderKind::Gemini]);

    let p1 = store.find_by_id_mut(&pid(ProviderKind::Claude)).unwrap();
    p1.connection = crate::models::ConnectionStatus::Connected;
    p1.quotas = vec![QuotaInfo::new("session", 10.0, 100.0)]; // Green

    // Gemini 已启用但未连接（无配额数据）→ 不参与聚合
    let p2 = store.find_by_id_mut(&pid(ProviderKind::Gemini)).unwrap();
    p2.connection = crate::models::ConnectionStatus::Disconnected;

    let settings = make_settings(&[ProviderKind::Claude, ProviderKind::Gemini]);
    let session = AppSession::new(settings, store.providers);
    assert_eq!(session.worst_enabled_provider_status(), StatusLevel::Green);
}

// ── AppSession::new 初始化 ──────────────────────────────

#[test]
fn session_new_defaults_to_overview_when_enabled_provider_exists() {
    let store = make_store(&[ProviderKind::Claude]);
    let settings = make_settings(&[ProviderKind::Claude]);

    let session = AppSession::new(settings, store.providers);

    assert_eq!(session.nav.active_tab, NavTab::Overview);
    assert_eq!(session.nav.last_provider_id, pid(ProviderKind::Claude));
}

#[test]
fn session_new_uses_first_enabled_provider_when_overview_hidden() {
    let store = make_store(&[ProviderKind::Claude, ProviderKind::Gemini]);
    let mut settings = make_settings(&[ProviderKind::Gemini]);
    settings.display.show_overview = false;

    let session = AppSession::new(settings, store.providers);

    assert_eq!(
        session.nav.active_tab,
        NavTab::Provider(pid(ProviderKind::Gemini))
    );
    assert_eq!(session.nav.last_provider_id, pid(ProviderKind::Gemini));
}

#[test]
fn session_new_uses_settings_tab_when_overview_hidden_and_no_enabled_provider() {
    let store = make_store(&[ProviderKind::Claude]);
    let mut settings = AppSettings::default();
    settings.display.show_overview = false;

    let session = AppSession::new(settings, store.providers);

    assert_eq!(session.nav.active_tab, NavTab::Settings);
    assert_eq!(session.nav.last_provider_id, pid(ProviderKind::Claude));
}

#[test]
fn session_new_selects_first_sidebar_provider_for_settings() {
    let store = make_store(&[ProviderKind::Claude, ProviderKind::Gemini]);
    let mut settings = make_settings(&[ProviderKind::Claude, ProviderKind::Gemini]);
    settings.provider.sidebar_providers = vec!["gemini".into(), "claude".into()];
    settings.provider.provider_order = vec!["gemini".into(), "claude".into()];

    let session = AppSession::new(settings, store.providers);

    assert_eq!(
        session.settings_ui.selected_provider,
        pid(ProviderKind::Gemini)
    );
    assert!(session.settings_ui.global_hotkey_error.is_none());
    assert!(session.settings_ui.global_hotkey_error_candidate.is_none());
}

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

#[test]
fn session_new_reuses_existing_sidebar_entry_for_custom_provider() {
    let mut store = make_store(&[ProviderKind::Claude]);
    let custom_id = ProviderId::Custom("my-relay:newapi".to_string());
    let metadata = crate::models::test_helpers::make_test_metadata(ProviderKind::Custom);
    store
        .providers
        .push(ProviderStatus::new(custom_id.clone(), metadata));

    let mut settings = make_settings(&[ProviderKind::Claude]);
    settings.provider.sidebar_providers.push(custom_id.id_key());

    let session = AppSession::new(settings, store.providers);

    assert!(session.settings.provider.is_enabled(&custom_id));
    assert_eq!(
        session
            .settings
            .provider
            .sidebar_providers
            .iter()
            .filter(|key| **key == "my-relay:newapi")
            .count(),
        1
    );
}

// ── ProviderKind::first() ─────────────────────────────────

#[test]
fn provider_kind_first_matches_all_index_zero() {
    assert_eq!(ProviderKind::first(), ProviderKind::all()[0]);
}

// ── AppSession::first_sidebar_provider() ──────────────────

#[test]
fn first_sidebar_provider_returns_first_in_sidebar() {
    let mut settings = make_settings(&[ProviderKind::Claude, ProviderKind::Gemini]);
    settings.provider.sidebar_providers = vec!["gemini".into(), "claude".into()];
    settings.provider.provider_order = vec!["gemini".into(), "claude".into()];

    let session = AppSession::new(
        settings,
        vec![
            make_provider(ProviderKind::Claude),
            make_provider(ProviderKind::Gemini),
        ],
    );

    assert_eq!(session.first_sidebar_provider(), pid(ProviderKind::Gemini));
}

#[test]
fn first_sidebar_provider_falls_back_to_manifest_first() {
    let settings = AppSettings::default(); // empty sidebar
    let session = AppSession::new(settings, vec![]);

    assert_eq!(session.first_sidebar_provider(), pid(ProviderKind::first()));
}
