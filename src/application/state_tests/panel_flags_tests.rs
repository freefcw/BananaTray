use super::super::*;
use crate::models::test_helpers::make_test_provider;
use crate::models::{ConnectionStatus, DisplaySettings, ProviderKind};

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
