use super::super::*;
use super::common::*;
use crate::models::{ConnectionStatus, NavTab, ProviderKind};

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
fn header_status_boundary_59s_is_synced() {
    let mut store = make_store(&[ProviderKind::Claude]);
    let p = store.find_by_id_mut(&pid(ProviderKind::Claude)).unwrap();
    p.connection = ConnectionStatus::Connected;
    p.last_refreshed_instant = Some(std::time::Instant::now() - std::time::Duration::from_secs(59));

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
fn header_status_boundary_60s_is_stale() {
    let mut store = make_store(&[ProviderKind::Claude]);
    let p = store.find_by_id_mut(&pid(ProviderKind::Claude)).unwrap();
    p.connection = ConnectionStatus::Connected;
    p.last_refreshed_instant = Some(std::time::Instant::now() - std::time::Duration::from_secs(60));

    let nav = NavigationState {
        active_tab: NavTab::Provider(pid(ProviderKind::Claude)),
        last_provider_id: pid(ProviderKind::Claude),
        prev_active_tab: None,
        generation: 0,
    };
    let (kind, elapsed) = compute_header_status(&nav, &store);
    assert_eq!(kind, HeaderStatusKind::Stale);
    assert!(elapsed.unwrap() >= 60);
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
