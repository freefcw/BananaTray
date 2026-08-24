use super::super::*;
use super::common::*;
use crate::application::{overview_view_state, OverviewItemStatus};
use crate::models::{
    FailureReason, NavTab, ProviderCapability, ProviderFailure, ProviderKind, QuotaInfo, QuotaType,
};

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

// ── Overview 面板高度（走 AppSession::popup_height 的真实分支）──
// AppSession::new 在 show_overview 默认开启时就落在 Overview tab

#[test]
fn popup_height_overview_single_provider_has_no_dead_space() {
    let store = make_store(&[ProviderKind::Claude]);
    // Overview 分支只统计已启用 Provider
    let session = AppSession::new(make_settings(&[ProviderKind::Claude]), store.providers);
    assert_eq!(session.nav.active_tab, NavTab::Overview);

    let h = session.popup_height();

    assert_eq!(h, crate::models::PopupLayout::MIN_OVERVIEW_HEIGHT);
    assert_eq!(
        h,
        crate::models::PopupLayout::FIXED_HEIGHT + crate::models::PopupLayout::OVERVIEW_ITEM_HEIGHT
    );
}

/// 展开态要让窗口长高，否则展开的内容会被压在折叠高度里滚动
#[test]
fn popup_height_overview_grows_when_expanded() {
    let mut store = make_store(&[ProviderKind::Claude]);
    store.providers[0].connection = crate::models::ConnectionStatus::Connected;
    store.providers[0].quotas = vec![
        QuotaInfo::new("session", 10.0, 100.0),
        QuotaInfo::new("weekly", 20.0, 100.0),
        QuotaInfo::new("monthly", 30.0, 100.0),
    ];
    let mut session = AppSession::new(make_settings(&[ProviderKind::Claude]), store.providers);

    let collapsed = session.popup_height();

    session.toggle_overview_expanded(&pid(ProviderKind::Claude));
    let expanded = session.popup_height();

    assert_eq!(
        expanded,
        crate::models::PopupLayout::FIXED_HEIGHT
            + crate::models::PopupLayout::overview_multi_item_height(3)
    );
    assert!(expanded > collapsed);
}

/// 展开态只按可见配额算行数，隐藏的配额不占高度
#[test]
fn popup_height_overview_expansion_ignores_hidden_quotas() {
    let mut store = make_store(&[ProviderKind::Claude]);
    store.providers[0].connection = crate::models::ConnectionStatus::Connected;
    store.providers[0].quotas = vec![
        QuotaInfo::new("session", 10.0, 100.0),
        QuotaInfo::new("weekly", 20.0, 100.0),
    ];
    let mut settings = make_settings(&[ProviderKind::Claude]);
    settings
        .provider
        .toggle_quota_visibility(&pid(ProviderKind::Claude), "weekly".to_string());
    let mut session = AppSession::new(settings, store.providers);
    session.toggle_overview_expanded(&pid(ProviderKind::Claude));

    let h = session.popup_height();

    // 只剩 1 个可见配额 → 退回单行卡片高度
    assert_eq!(h, crate::models::PopupLayout::MIN_OVERVIEW_HEIGHT);
}

/// 刷新和断开状态只渲染一行提示，即使缓存配额和展开记忆仍然存在
#[test]
fn popup_height_overview_status_rows_ignore_cached_expansion() {
    let mut store = make_store(&[ProviderKind::Claude]);
    store.providers[0].connection = crate::models::ConnectionStatus::Connected;
    store.providers[0].quotas = vec![
        QuotaInfo::new("session", 10.0, 100.0),
        QuotaInfo::new("weekly", 20.0, 100.0),
        QuotaInfo::new("monthly", 30.0, 100.0),
    ];
    let provider_id = pid(ProviderKind::Claude);
    let mut session = AppSession::new(make_settings(&[ProviderKind::Claude]), store.providers);
    session.toggle_overview_expanded(&provider_id);

    session.provider_store.mark_refreshing_by_id(&provider_id);
    assert!(matches!(
        overview_view_state(&session).items[0].status,
        OverviewItemStatus::Refreshing
    ));
    assert_eq!(
        session.popup_height(),
        crate::models::PopupLayout::MIN_OVERVIEW_HEIGHT
    );

    session
        .provider_store
        .find_by_id_mut(&provider_id)
        .unwrap()
        .mark_unavailable(ProviderFailure {
            reason: FailureReason::Unavailable,
            advice: None,
            raw_detail: None,
        });
    assert!(matches!(
        overview_view_state(&session).items[0].status,
        OverviewItemStatus::Disconnected
    ));
    assert_eq!(
        session.popup_height(),
        crate::models::PopupLayout::MIN_OVERVIEW_HEIGHT
    );
}

/// 不参与监控的 Provider 始终渲染单行说明，不受缓存配额和展开记忆影响
#[test]
fn popup_height_overview_non_monitorable_card_stays_single_row() {
    let mut store = make_store(&[ProviderKind::Claude]);
    store.providers[0].connection = crate::models::ConnectionStatus::Connected;
    store.providers[0].provider_capability = ProviderCapability::Informational;
    store.providers[0].quotas = vec![
        QuotaInfo::new("session", 10.0, 100.0),
        QuotaInfo::new("weekly", 20.0, 100.0),
    ];
    let provider_id = pid(ProviderKind::Claude);
    let mut session = AppSession::new(make_settings(&[ProviderKind::Claude]), store.providers);
    session.toggle_overview_expanded(&provider_id);

    assert!(matches!(
        overview_view_state(&session).items[0].status,
        OverviewItemStatus::Error { .. }
    ));
    assert_eq!(
        session.popup_height(),
        crate::models::PopupLayout::MIN_OVERVIEW_HEIGHT
    );
}
