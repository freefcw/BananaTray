use super::common::{has_effect, has_render, make_session, pid};
use crate::application::{
    reduce, AppAction, AppEffect, CommonEffect, ContextEffect, GlobalHotkeyError,
    NotificationEffect, RefreshEffect, SettingChange, SettingsEffect, TrayIconRequest,
};
use crate::models::{NavTab, ProviderKind, RefreshData};
use crate::refresh::{RefreshEvent, RefreshOutcome, RefreshResult};

// ── Cadence Dropdown ────────────────────────────────

#[test]
fn toggle_cadence_dropdown_flips_state() {
    let mut session = make_session();
    assert!(!session.settings_ui.cadence_dropdown_open);

    let effects = reduce(&mut session, AppAction::ToggleCadenceDropdown);

    assert!(session.settings_ui.cadence_dropdown_open);
    assert!(has_render(&effects));

    reduce(&mut session, AppAction::ToggleCadenceDropdown);
    assert!(!session.settings_ui.cadence_dropdown_open);
}

#[test]
fn update_refresh_cadence_applies_setting_and_closes_dropdown() {
    let mut session = make_session();
    session.settings_ui.cadence_dropdown_open = true;

    let effects = reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::RefreshCadence(Some(15))),
    );

    assert_eq!(session.settings.system.refresh_interval_mins, 15);
    assert!(!session.settings_ui.cadence_dropdown_open);
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::PersistSettings))
    )));
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Refresh(RefreshEffect::SendRequest(_)))
    )));
    assert!(has_render(&effects));
}

#[test]
fn save_global_hotkey_emits_runtime_effect_and_clears_error() {
    let mut session = make_session();
    session.settings_ui.global_hotkey_error = Some(GlobalHotkeyError::InvalidFormat);
    session.settings_ui.global_hotkey_error_candidate = Some("cmd-shift-j".to_string());

    let effects = reduce(
        &mut session,
        AppAction::SaveGlobalHotkey("Cmd+Shift+K".to_string()),
    );

    assert!(session.settings_ui.global_hotkey_error.is_none());
    assert!(session.settings_ui.global_hotkey_error_candidate.is_none());
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Context(ContextEffect::ApplyGlobalHotkey(hotkey))
            if hotkey == "Cmd+Shift+K"
    )));
    assert!(has_render(&effects));
}

// ── ToggleStartAtLogin ───────────────────────────────

#[test]
fn toggle_start_at_login_produces_sync_and_notification() {
    let mut session = make_session();
    assert!(!session.settings.system.start_at_login);

    let effects = reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::ToggleStartAtLogin),
    );

    assert!(session.settings.system.start_at_login);
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::SyncAutoLaunch(true)))
    )));
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Notification(NotificationEffect::Plain { .. }))
    )));
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::PersistSettings))
    )));
    assert!(has_render(&effects));
}

#[test]
fn toggle_start_at_login_round_trip() {
    let mut session = make_session();

    // enable
    let effects = reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::ToggleStartAtLogin),
    );
    assert!(session.settings.system.start_at_login);
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::SyncAutoLaunch(true)))
    )));

    // disable
    let effects = reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::ToggleStartAtLogin),
    );
    assert!(!session.settings.system.start_at_login);
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::SyncAutoLaunch(
            false
        )))
    )));
}

// ── ToggleShowAccountInfo ───────────────────────────

#[test]
fn toggle_show_account_info_flips_setting() {
    let mut session = make_session();
    assert!(session.settings.display.show_account_info); // default = true

    let effects = reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::ToggleShowAccountInfo),
    );

    assert!(!session.settings.display.show_account_info);
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::PersistSettings))
    )));
    assert!(has_render(&effects));
}

#[test]
fn toggle_show_account_info_round_trip() {
    let mut session = make_session();

    reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::ToggleShowAccountInfo),
    );
    assert!(!session.settings.display.show_account_info);

    reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::ToggleShowAccountInfo),
    );
    assert!(session.settings.display.show_account_info);
}

// ── ToggleShowOverview ──────────────────────────────

#[test]
fn toggle_show_overview_off_switches_to_first_enabled_provider() {
    let mut session = make_session();
    session
        .settings
        .provider
        .set_enabled(&pid(ProviderKind::Claude), true);
    assert_eq!(session.nav.active_tab, NavTab::Overview);

    reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::ToggleShowOverview),
    );

    assert!(!session.settings.display.show_overview);
    assert!(matches!(session.nav.active_tab, NavTab::Provider(_)));
}

#[test]
fn toggle_show_overview_off_with_all_disabled_stays_on_overview() {
    let mut session = make_session();
    // 所有 provider 默认禁用，无需额外设置
    session.nav.switch_to(NavTab::Overview);

    reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::ToggleShowOverview),
    );

    assert!(!session.settings.display.show_overview);
    // default_provider_tab() 返回 None，tab 不切换
    assert_eq!(session.nav.active_tab, NavTab::Overview);
}

#[test]
fn toggle_show_overview_round_trip() {
    let mut session = make_session();
    session
        .settings
        .provider
        .set_enabled(&pid(ProviderKind::Claude), true);
    let initial_tab = session.nav.active_tab.clone();

    // 关闭 Overview
    reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::ToggleShowOverview),
    );
    assert!(!session.settings.display.show_overview);
    let tab_after_close = session.nav.active_tab.clone();
    assert_ne!(tab_after_close, initial_tab); // 应该切换了

    // 重新打开 Overview
    reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::ToggleShowOverview),
    );
    assert!(session.settings.display.show_overview);
    // 打开 Overview 不影响 active_tab
    assert_eq!(session.nav.active_tab, tab_after_close);
}

// ── SetTrayIconStyle ───────────────────────────────

#[test]
fn set_tray_icon_style_updates_setting_and_produces_effects() {
    use crate::models::TrayIconStyle;

    let mut session = make_session();
    assert_eq!(
        session.settings.display.tray_icon_style,
        TrayIconStyle::default()
    );

    let effects = reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::SetTrayIconStyle(TrayIconStyle::Colorful)),
    );

    assert_eq!(
        session.settings.display.tray_icon_style,
        TrayIconStyle::Colorful
    );
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Context(ContextEffect::ApplyTrayIcon(TrayIconRequest::Static(
            TrayIconStyle::Colorful
        )))
    )));
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::PersistSettings))
    )));
    assert!(has_render(&effects));
}

#[test]
fn set_tray_icon_style_round_trip() {
    use crate::models::TrayIconStyle;

    let mut session = make_session();

    reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::SetTrayIconStyle(TrayIconStyle::Colorful)),
    );
    assert_eq!(
        session.settings.display.tray_icon_style,
        TrayIconStyle::Colorful
    );

    reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::SetTrayIconStyle(TrayIconStyle::Monochrome)),
    );
    assert_eq!(
        session.settings.display.tray_icon_style,
        TrayIconStyle::Monochrome
    );
}

#[test]
fn set_tray_icon_dynamic_produces_dynamic_status_effect() {
    use crate::models::{StatusLevel, TrayIconStyle};

    let mut session = make_session();
    let effects = reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::SetTrayIconStyle(TrayIconStyle::Dynamic)),
    );

    assert_eq!(
        session.settings.display.tray_icon_style,
        TrayIconStyle::Dynamic
    );
    // 无已连接 provider → DynamicStatus(Green)
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Context(ContextEffect::ApplyTrayIcon(
            TrayIconRequest::DynamicStatus(StatusLevel::Green)
        ))
    )));
}

// ── PopupVisibilityChanged ────────────────────────────

#[test]
fn popup_closed_syncs_dynamic_icon() {
    use crate::models::{QuotaInfo, StatusLevel, TrayIconStyle};

    let mut session = make_session();
    session.settings.display.tray_icon_style = TrayIconStyle::Dynamic;
    session.popup_visible = true;

    // 先让 Claude 有 Red 数据（在弹窗打开期间刷新，图标未更新）
    reduce(
        &mut session,
        AppAction::RefreshEventReceived(RefreshEvent::Finished(RefreshOutcome {
            id: pid(ProviderKind::Claude),
            result: RefreshResult::Success {
                data: RefreshData {
                    quotas: vec![QuotaInfo::new("session", 95.0, 100.0)],
                    account_email: None,
                    account_tier: None,
                    source_label: None,
                },
            },
        })),
    );

    // 关闭弹窗 → 同步图标
    let effects = reduce(&mut session, AppAction::PopupVisibilityChanged(false));

    assert!(!session.popup_visible);
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Context(ContextEffect::ApplyTrayIcon(
            TrayIconRequest::DynamicStatus(StatusLevel::Red)
        ))
    )));
}

#[test]
fn popup_opened_sets_flag_no_icon_effect() {
    use crate::models::TrayIconStyle;

    let mut session = make_session();
    session.settings.display.tray_icon_style = TrayIconStyle::Dynamic;

    let effects = reduce(&mut session, AppAction::PopupVisibilityChanged(true));

    assert!(session.popup_visible);
    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Context(ContextEffect::ApplyTrayIcon(_))
    )));
}

#[test]
fn popup_closed_in_static_mode_no_icon_effect() {
    use crate::models::TrayIconStyle;

    let mut session = make_session();
    session.settings.display.tray_icon_style = TrayIconStyle::Monochrome;
    session.popup_visible = true;

    let effects = reduce(&mut session, AppAction::PopupVisibilityChanged(false));

    assert!(!session.popup_visible);
    // 非 Dynamic 模式不产出图标更新
    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Context(ContextEffect::ApplyTrayIcon(_))
    )));
}

// ── SetQuotaDisplayMode ────────────────────────────

#[test]
fn set_quota_display_mode_updates_setting_and_produces_effects() {
    use crate::models::QuotaDisplayMode;

    let mut session = make_session();
    assert_eq!(
        session.settings.display.quota_display_mode,
        QuotaDisplayMode::Remaining
    );

    let effects = reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::SetQuotaDisplayMode(QuotaDisplayMode::Used)),
    );

    assert_eq!(
        session.settings.display.quota_display_mode,
        QuotaDisplayMode::Used
    );
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::PersistSettings))
    )));
    assert!(has_render(&effects));
}

#[test]
fn set_quota_display_mode_round_trip() {
    use crate::models::QuotaDisplayMode;

    let mut session = make_session();

    reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::SetQuotaDisplayMode(QuotaDisplayMode::Used)),
    );
    assert_eq!(
        session.settings.display.quota_display_mode,
        QuotaDisplayMode::Used
    );

    reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::SetQuotaDisplayMode(
            QuotaDisplayMode::Remaining,
        )),
    );
    assert_eq!(
        session.settings.display.quota_display_mode,
        QuotaDisplayMode::Remaining
    );
}

// ── ToggleQuotaVisibility ──────────────────────────

#[test]
fn toggle_quota_visibility_updates_setting_and_produces_effects() {
    let mut session = make_session();
    let provider_id = pid(ProviderKind::Claude);
    assert!(session
        .settings
        .provider
        .is_quota_visible(&provider_id, "session"));

    let effects = reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::ToggleQuotaVisibility {
            provider_id: provider_id.clone(),
            quota_key: "session".to_string(),
        }),
    );

    assert!(!session
        .settings
        .provider
        .is_quota_visible(&provider_id, "session"));
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::PersistSettings))
    )));
    assert!(has_render(&effects));
}

#[test]
fn toggle_quota_visibility_round_trip() {
    let mut session = make_session();
    let provider_id = pid(ProviderKind::Claude);

    reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::ToggleQuotaVisibility {
            provider_id: provider_id.clone(),
            quota_key: "weekly".to_string(),
        }),
    );
    assert!(!session
        .settings
        .provider
        .is_quota_visible(&provider_id, "weekly"));

    reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::ToggleQuotaVisibility {
            provider_id,
            quota_key: "weekly".to_string(),
        }),
    );
    assert!(session
        .settings
        .provider
        .is_quota_visible(&pid(ProviderKind::Claude), "weekly"));
}
