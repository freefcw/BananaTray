use super::super::*;
use super::common::*;
use crate::models::{NavTab, ProviderKind, QuotaInfo, QuotaType};

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
        .toggle_quota_visibility(&pid(ProviderKind::Claude), "session".to_string());

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
