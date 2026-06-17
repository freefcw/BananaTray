use super::super::*;
use super::common::*;
use crate::models::{NavTab, ProviderKind};

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
