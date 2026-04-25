use crate::application::{
    reduce, AppAction, AppEffect, AppSession, CommonEffect, ContextEffect, DebugEffect,
    GlobalHotkeyError, NewApiEffect, NotificationEffect, RefreshEffect, SettingChange,
    SettingsEffect, SettingsTab, TrayIconRequest,
};
use crate::models::test_helpers::make_test_provider;
use crate::models::{
    AppSettings, ConnectionStatus, NavTab, ProviderId, ProviderKind, RefreshData,
    SettingsCapability, TokenInputCapability,
};
use crate::refresh::{RefreshEvent, RefreshOutcome, RefreshRequest, RefreshResult};

fn pid(kind: ProviderKind) -> ProviderId {
    ProviderId::BuiltIn(kind)
}

fn make_session() -> AppSession {
    let providers = ProviderKind::all()
        .iter()
        .map(|k| make_test_provider(*k, ConnectionStatus::Disconnected))
        .collect();
    AppSession::new(AppSettings::default(), providers)
}

/// 构建一个不包含指定 provider 的 session
fn make_session_without(excluded: ProviderKind) -> AppSession {
    let providers = ProviderKind::all()
        .iter()
        .filter(|k| **k != excluded)
        .map(|k| make_test_provider(*k, ConnectionStatus::Disconnected))
        .collect();
    AppSession::new(AppSettings::default(), providers)
}

fn make_custom_token_provider(
    id: &str,
    credential_key: &'static str,
) -> crate::models::ProviderStatus {
    let provider_id = ProviderId::Custom(id.to_string());
    let mut metadata = crate::models::test_helpers::make_test_metadata(ProviderKind::Custom);
    metadata.display_name = "Custom Token".to_string();
    let mut status = crate::models::ProviderStatus::new(provider_id, metadata);
    status.settings_capability = SettingsCapability::TokenInput(TokenInputCapability {
        credential_key,
        placeholder_i18n_key: "copilot.token_placeholder",
        help_tip_i18n_key: "copilot.token_sources_tip",
        title_i18n_key: "copilot.github_login",
        description_i18n_key: "copilot.requires_auth",
        create_url: "https://example.com/token",
    });
    status
}

fn has_effect(effects: &[AppEffect], f: impl Fn(&AppEffect) -> bool) -> bool {
    effects.iter().any(f)
}

fn has_render(effects: &[AppEffect]) -> bool {
    has_effect(effects, |e| {
        matches!(e, AppEffect::Context(ContextEffect::Render))
    })
}

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

// ── SelectDebugProvider ─────────────────────────────

#[test]
fn select_debug_provider_updates_state() {
    let mut session = make_session();
    assert!(session.debug_ui.selected_provider.is_none());

    let effects = reduce(
        &mut session,
        AppAction::SelectDebugProvider(pid(ProviderKind::Claude)),
    );

    assert_eq!(
        session.debug_ui.selected_provider,
        Some(pid(ProviderKind::Claude))
    );
    assert!(has_render(&effects));
}

#[test]
fn select_debug_provider_can_change() {
    let mut session = make_session();
    reduce(
        &mut session,
        AppAction::SelectDebugProvider(pid(ProviderKind::Claude)),
    );
    reduce(
        &mut session,
        AppAction::SelectDebugProvider(pid(ProviderKind::Copilot)),
    );

    assert_eq!(
        session.debug_ui.selected_provider,
        Some(pid(ProviderKind::Copilot))
    );
}

#[test]
fn open_url_produces_context_effect() {
    let mut session = make_session();

    let effects = reduce(
        &mut session,
        AppAction::OpenUrl("https://example.com".to_string()),
    );

    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Context(ContextEffect::OpenUrl(url)) if url == "https://example.com"
    )));
}

#[test]
fn quit_app_produces_context_effect() {
    let mut session = make_session();

    let effects = reduce(&mut session, AppAction::QuitApp);

    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Context(ContextEffect::QuitApp)
    )));
}

// ── DebugRefreshProvider ────────────────────────────

#[test]
fn debug_refresh_without_selection_is_noop() {
    let mut session = make_session();
    let effects = reduce(&mut session, AppAction::DebugRefreshProvider);

    assert!(!session.debug_ui.refresh_active);
    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Debug(DebugEffect::StartRefresh(_)))
    )));
}

#[test]
fn debug_refresh_with_selection_produces_effect() {
    let mut session = make_session();
    reduce(
        &mut session,
        AppAction::SelectDebugProvider(pid(ProviderKind::Gemini)),
    );

    let effects = reduce(&mut session, AppAction::DebugRefreshProvider);

    assert!(session.debug_ui.refresh_active);
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Debug(DebugEffect::StartRefresh(_)))
    )));
}

#[test]
fn debug_refresh_non_monitorable_provider_is_noop() {
    let mut session = make_session();
    reduce(
        &mut session,
        AppAction::SelectDebugProvider(pid(ProviderKind::Kilo)),
    );

    let effects = reduce(&mut session, AppAction::DebugRefreshProvider);

    assert!(!session.debug_ui.refresh_active);
    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Debug(DebugEffect::StartRefresh(_)))
    )));
}

#[test]
fn toggle_non_monitorable_provider_on_renders_without_refresh_request() {
    let mut session = make_session();
    let id = pid(ProviderKind::Kilo);

    let effects = reduce(&mut session, AppAction::ToggleProvider(id.clone()));

    assert!(session.settings.provider.is_enabled(&id));
    assert_eq!(session.nav.active_tab, NavTab::Provider(id));
    assert!(has_render(&effects));
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Refresh(RefreshEffect::SendRequest(
            RefreshRequest::UpdateConfig { .. }
        )))
    )));
    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Refresh(RefreshEffect::SendRequest(
            RefreshRequest::RefreshOne { .. }
        )))
    )));
}

#[test]
fn debug_refresh_while_active_is_noop() {
    let mut session = make_session();
    reduce(
        &mut session,
        AppAction::SelectDebugProvider(pid(ProviderKind::Gemini)),
    );
    reduce(&mut session, AppAction::DebugRefreshProvider);

    // 再次点击不应重复触发
    let effects = reduce(&mut session, AppAction::DebugRefreshProvider);
    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Debug(DebugEffect::StartRefresh(_)))
    )));
}

// ── SetTrayIconStyle ───────────────────────────────

#[test]
fn set_tray_icon_style_updates_setting_and_produces_effects() {
    use crate::models::TrayIconStyle;

    let mut session = make_session();
    assert_eq!(
        session.settings.display.tray_icon_style,
        TrayIconStyle::Monochrome
    );

    let effects = reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::SetTrayIconStyle(TrayIconStyle::Yellow)),
    );

    assert_eq!(
        session.settings.display.tray_icon_style,
        TrayIconStyle::Yellow
    );
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Context(ContextEffect::ApplyTrayIcon(TrayIconRequest::Static(
            TrayIconStyle::Yellow
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

#[test]
fn refresh_success_in_dynamic_mode_produces_tray_icon_effect() {
    use crate::models::{QuotaInfo, StatusLevel, TrayIconStyle};

    let mut session = make_session();
    session.settings.display.tray_icon_style = TrayIconStyle::Dynamic;
    session
        .settings
        .provider
        .set_enabled(&pid(ProviderKind::Claude), true);
    // make_session 的 last_provider_id = Claude

    let effects = reduce(
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

    // 当前 Provider Claude 变 Red → 产出 ApplyTrayIcon
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Context(ContextEffect::ApplyTrayIcon(
            TrayIconRequest::DynamicStatus(StatusLevel::Red)
        ))
    )));
}

#[test]
fn refresh_success_in_static_mode_does_not_produce_tray_icon_effect() {
    use crate::models::{QuotaInfo, TrayIconStyle};

    let mut session = make_session();
    session.settings.display.tray_icon_style = TrayIconStyle::Yellow; // 静态模式
    session
        .settings
        .provider
        .set_enabled(&pid(ProviderKind::Claude), true);

    let effects = reduce(
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

    // 静态模式下不应产出 ApplyTrayIcon effect
    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Context(ContextEffect::ApplyTrayIcon(_))
    )));
}

#[test]
fn refresh_success_in_dynamic_mode_no_effect_when_status_unchanged() {
    use crate::models::{QuotaInfo, TrayIconStyle};

    let mut session = make_session();
    session.settings.display.tray_icon_style = TrayIconStyle::Dynamic;
    session
        .settings
        .provider
        .set_enabled(&pid(ProviderKind::Claude), true);

    // 第一次刷新：Green → Red，产出 effect
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

    // 第二次刷新：Red → Red（状态不变），不应产出 ApplyTrayIcon
    let effects = reduce(
        &mut session,
        AppAction::RefreshEventReceived(RefreshEvent::Finished(RefreshOutcome {
            id: pid(ProviderKind::Claude),
            result: RefreshResult::Success {
                data: RefreshData {
                    quotas: vec![QuotaInfo::new("session", 96.0, 100.0)], // 仍是 Red
                    account_email: None,
                    account_tier: None,
                    source_label: None,
                },
            },
        })),
    );

    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Context(ContextEffect::ApplyTrayIcon(_))
    )));
}

#[test]
fn refresh_non_current_provider_does_not_produce_tray_icon_effect() {
    use crate::models::{QuotaInfo, TrayIconStyle};

    let mut session = make_session();
    session.settings.display.tray_icon_style = TrayIconStyle::Dynamic;
    // 当前 Provider 是 Claude（默认），但刷新的是 Gemini

    let effects = reduce(
        &mut session,
        AppAction::RefreshEventReceived(RefreshEvent::Finished(RefreshOutcome {
            id: pid(ProviderKind::Gemini),
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

    // 非当前 Provider 的刷新不影响图标
    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Context(ContextEffect::ApplyTrayIcon(_))
    )));
}

#[test]
fn refresh_deferred_while_popup_visible() {
    use crate::models::{QuotaInfo, TrayIconStyle};

    let mut session = make_session();
    session.settings.display.tray_icon_style = TrayIconStyle::Dynamic;
    session.popup_visible = true; // 弹窗打开

    let effects = reduce(
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

    // 弹窗可见时不产出 ApplyTrayIcon
    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Context(ContextEffect::ApplyTrayIcon(_))
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

// ── NewAPI 快速添加 ────────────────────────────────

#[test]
fn enter_add_newapi_sets_flag_true() {
    let mut session = make_session();
    assert!(!session.settings_ui.adding_newapi);

    let effects = reduce(&mut session, AppAction::EnterAddNewApi);

    assert!(session.settings_ui.adding_newapi);
    assert!(has_render(&effects));
}

#[test]
fn cancel_add_newapi_resets_flag() {
    let mut session = make_session();
    session.settings_ui.adding_newapi = true;

    let effects = reduce(&mut session, AppAction::CancelAddNewApi);

    assert!(!session.settings_ui.adding_newapi);
    assert!(has_render(&effects));
}

#[test]
fn submit_newapi_produces_save_and_notification_effects() {
    let mut session = make_session();
    session.settings_ui.adding_newapi = true;

    let effects = reduce(
        &mut session,
        AppAction::SubmitNewApi {
            display_name: "Test Site".to_string(),
            base_url: "https://api.example.com".to_string(),
            cookie: "session=tok_123".to_string(),
            user_id: Some("42".to_string()),
            divisor: Some(1_000_000.0),
        },
    );

    // 状态：表单已关闭
    assert!(!session.settings_ui.adding_newapi);

    // Effect: NewApiEffect::SaveProvider（检查 config 包含关键字段 + 新增模式）
    assert!(has_effect(&effects, |e| {
        matches!(e, AppEffect::Common(CommonEffect::NewApi(NewApiEffect::SaveProvider { config, is_editing, .. }))
            if config.display_name == "Test Site"
            && config.base_url == "https://api.example.com"
            && config.cookie == "session=tok_123"
            && config.user_id == Some("42".to_string())
            && config.divisor == Some(1_000_000.0)
            && !is_editing
        )
    }));

    // SettingsEffect::PersistSettings 和 NotificationEffect::Plain 已移至 runtime 成功路径
    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::PersistSettings))
    )));

    assert!(has_render(&effects));
}

#[test]
fn submit_newapi_auto_enables_and_adds_to_sidebar() {
    let mut session = make_session();
    session.settings_ui.adding_newapi = true;
    // sidebar 初始为空（模拟全新用户场景）
    session.settings.provider.sidebar_providers = vec!["claude".into()];

    let effects = reduce(
        &mut session,
        AppAction::SubmitNewApi {
            display_name: "My Relay".to_string(),
            base_url: "https://relay.example.com".to_string(),
            cookie: "c=1".to_string(),
            user_id: None,
            divisor: None,
        },
    );

    let expected_id = ProviderId::Custom("relay-example-com:newapi".to_string());

    // 自动启用
    assert!(session.settings.provider.is_enabled(&expected_id));
    // 加入 sidebar
    assert!(session
        .settings
        .provider
        .sidebar_providers
        .contains(&"relay-example-com:newapi".to_string()));
    // 设置页选中新 Provider
    assert_eq!(session.settings_ui.selected_provider, expected_id);
    // PersistSettings 已移至 runtime 成功路径，reducer 不再发射
    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::PersistSettings))
    )));
}

#[test]
fn submit_newapi_edit_mode_preserves_existing_enabled_state() {
    use crate::models::NewApiEditData;

    let mut session = make_session();
    let custom_id = ProviderId::Custom("old-site-com:newapi".to_string());
    session.settings.provider.set_enabled(&custom_id, true);
    session
        .settings
        .provider
        .sidebar_providers
        .push("old-site-com:newapi".to_string());

    session.settings_ui.adding_newapi = true;
    session.settings_ui.editing_newapi = Some(NewApiEditData {
        display_name: "Old Site".to_string(),
        base_url: "https://old-site.com".to_string(),
        cookie: "c=old".to_string(),
        user_id: None,
        divisor: None,
        original_filename: "original.yaml".to_string(),
    });

    reduce(
        &mut session,
        AppAction::SubmitNewApi {
            display_name: "Updated Name".to_string(),
            base_url: "https://old-site.com".to_string(), // URL 不变
            cookie: "c=new".to_string(),
            user_id: None,
            divisor: None,
        },
    );

    // 已存在的 enabled 状态不被覆盖
    assert!(session.settings.provider.is_enabled(&custom_id));
}

#[test]
fn submit_newapi_reenables_same_provider_after_create_rollback() {
    let mut session = make_session();
    session.settings_ui.adding_newapi = true;

    let base_url = "https://retry.example.com";
    let retry_id = ProviderId::Custom("retry-example-com:newapi".to_string());

    reduce(
        &mut session,
        AppAction::SubmitNewApi {
            display_name: "Retry Relay".to_string(),
            base_url: base_url.to_string(),
            cookie: "c=1".to_string(),
            user_id: None,
            divisor: None,
        },
    );
    assert!(session.settings.provider.is_enabled(&retry_id));

    crate::application::newapi_ops::rollback_newapi_create(
        &mut session,
        &crate::models::NewApiConfig {
            display_name: "Retry Relay".to_string(),
            base_url: base_url.to_string(),
            cookie: "c=1".to_string(),
            user_id: None,
            divisor: None,
        },
    );
    assert!(!session
        .settings
        .provider
        .enabled_providers
        .contains_key(&retry_id.id_key()));

    reduce(
        &mut session,
        AppAction::SubmitNewApi {
            display_name: "Retry Relay".to_string(),
            base_url: base_url.to_string(),
            cookie: "c=2".to_string(),
            user_id: None,
            divisor: None,
        },
    );

    assert!(session.settings.provider.is_enabled(&retry_id));
    assert!(session
        .settings
        .provider
        .sidebar_providers
        .contains(&retry_id.id_key()));
    assert_eq!(session.settings_ui.selected_provider, retry_id);
}

#[test]
fn providers_reloaded_auto_enables_new_custom_provider() {
    let mut session = make_session();

    // settings 中没有 "fresh:api" 的任何条目
    let mut statuses = session.provider_store.providers.to_vec();
    statuses.push(make_custom_provider_status("fresh:api"));

    let effects = reduce(
        &mut session,
        AppAction::RefreshEventReceived(RefreshEvent::ProvidersReloaded { statuses }),
    );

    let fresh_id = ProviderId::Custom("fresh:api".to_string());
    // 自动启用
    assert!(session.settings.provider.is_enabled(&fresh_id));
    // 加入 sidebar
    assert!(session
        .settings
        .provider
        .sidebar_providers
        .contains(&"fresh:api".to_string()));
    // 产出 PersistSettings
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::PersistSettings))
    )));
    // 触发立即刷新
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Refresh(RefreshEffect::SendRequest(RefreshRequest::RefreshOne {
            ref id,
            ..
        }))) if *id == fresh_id
    )));
}

#[test]
fn providers_reloaded_reuses_existing_sidebar_entry_for_new_custom_provider() {
    let mut session = make_session();
    session
        .settings
        .provider
        .sidebar_providers
        .push("fresh:api".to_string());

    let mut statuses = session.provider_store.providers.to_vec();
    statuses.push(make_custom_provider_status("fresh:api"));

    let effects = reduce(
        &mut session,
        AppAction::RefreshEventReceived(RefreshEvent::ProvidersReloaded { statuses }),
    );

    let fresh_id = ProviderId::Custom("fresh:api".to_string());
    assert!(session.settings.provider.is_enabled(&fresh_id));
    assert_eq!(
        session
            .settings
            .provider
            .sidebar_providers
            .iter()
            .filter(|key| **key == "fresh:api")
            .count(),
        1
    );
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::PersistSettings))
    )));
}

#[test]
fn submit_newapi_without_optional_fields_uses_defaults() {
    let mut session = make_session();

    let effects = reduce(
        &mut session,
        AppAction::SubmitNewApi {
            display_name: "Minimal".to_string(),
            base_url: "https://minimal.io".to_string(),
            cookie: "session=abc".to_string(),
            user_id: None,
            divisor: None,
        },
    );

    assert!(has_effect(&effects, |e| {
        matches!(e, AppEffect::Common(CommonEffect::NewApi(NewApiEffect::SaveProvider { config, is_editing, .. }))
            if config.base_url == "https://minimal.io"
            && config.divisor.is_none()
            && !is_editing
        )
    }));
}

#[test]
fn select_provider_is_noop_during_newapi_form() {
    // 中转站表单打开时，侧栏点击应完全忽略：
    // 不修改 selected_provider，避免侧栏高亮与表单编辑目标不一致的分叉状态
    let mut session = make_session();
    session.settings_ui.adding_newapi = true;
    let original_selected = session.settings_ui.selected_provider.clone();

    let other_id = session.provider_store.providers[1].provider_id.clone();
    assert_ne!(original_selected, other_id); // 确保测试有意义

    let effects = reduce(&mut session, AppAction::SelectSettingsProvider(other_id));

    assert!(session.settings_ui.adding_newapi); // 表单保留
    assert_eq!(session.settings_ui.selected_provider, original_selected); // 选中不变
    assert!(effects.is_empty()); // 完全 no-op
}

#[test]
fn select_provider_clears_adding_provider() {
    // 添加内置服务商的 picker 是轻量操作，点选已有服务商应退出
    let mut session = make_session();
    session.settings_ui.adding_provider = true;

    let id = session.provider_store.providers[0].provider_id.clone();
    let effects = reduce(&mut session, AppAction::SelectSettingsProvider(id));

    assert!(!session.settings_ui.adding_provider); // picker 已退出
    assert!(has_render(&effects));
}

#[test]
fn set_settings_tab_clears_adding_provider() {
    // 切换 tab 时应退出 picker
    let mut session = make_session();
    session.settings_ui.adding_provider = true;

    let effects = reduce(
        &mut session,
        AppAction::SetSettingsTab(SettingsTab::General),
    );

    assert!(!session.settings_ui.adding_provider);
    assert!(has_render(&effects));
}

#[test]
fn set_settings_tab_preserves_adding_newapi() {
    // 中转站表单是复杂操作，切换 tab 不应丢失表单状态；
    // 用户切回 Providers tab 时应恢复表单界面
    let mut session = make_session();
    session.settings_ui.adding_newapi = true;

    reduce(
        &mut session,
        AppAction::SetSettingsTab(SettingsTab::General),
    );

    assert!(session.settings_ui.adding_newapi); // 表单状态保留
}

// ── 编辑模式 ──────────────────────────────────────

#[test]
fn submit_newapi_in_edit_mode_uses_original_filename() {
    use crate::models::NewApiEditData;

    let mut session = make_session();
    session.settings_ui.adding_newapi = true;
    session.settings_ui.editing_newapi = Some(NewApiEditData {
        display_name: "Old Name".to_string(),
        base_url: "https://old-site.com".to_string(),
        cookie: "old_cookie".to_string(),
        user_id: None,
        divisor: None,
        original_filename: "original-file.yaml".to_string(),
    });

    let effects = reduce(
        &mut session,
        AppAction::SubmitNewApi {
            display_name: "Updated Name".to_string(),
            base_url: "https://old-site.com".to_string(),
            cookie: "new_cookie".to_string(),
            user_id: Some("99".to_string()),
            divisor: Some(1_000_000.0),
        },
    );

    // 状态：编辑模式已清除
    assert!(!session.settings_ui.adding_newapi);
    assert!(session.settings_ui.editing_newapi.is_none());

    // Effect: 使用原始文件名 + 编辑模式标志
    assert!(has_effect(&effects, |e| {
        matches!(e, AppEffect::Common(CommonEffect::NewApi(NewApiEffect::SaveProvider { config, original_filename, is_editing }))
            if *original_filename == Some("original-file.yaml".to_string())
            && config.display_name == "Updated Name"
            && config.cookie == "new_cookie"
            && *is_editing
        )
    }));

    // SettingsEffect::PersistSettings 和通知已移至 runtime 成功路径
    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Notification(NotificationEffect::Plain { .. }))
    )));
}

#[test]
fn cancel_add_newapi_clears_editing_state() {
    use crate::models::NewApiEditData;

    let mut session = make_session();
    session.settings_ui.adding_newapi = true;
    session.settings_ui.editing_newapi = Some(NewApiEditData {
        display_name: "Test".to_string(),
        base_url: "https://test.com".to_string(),
        cookie: "c".to_string(),
        user_id: None,
        divisor: None,
        original_filename: "test.yaml".to_string(),
    });

    let effects = reduce(&mut session, AppAction::CancelAddNewApi);

    assert!(!session.settings_ui.adding_newapi);
    assert!(session.settings_ui.editing_newapi.is_none());
    assert!(has_render(&effects));
}

#[test]
fn enter_add_newapi_clears_stale_editing_state() {
    use crate::models::NewApiEditData;

    let mut session = make_session();
    // 模拟残留的编辑状态
    session.settings_ui.editing_newapi = Some(NewApiEditData {
        display_name: "Stale".to_string(),
        base_url: "https://stale.com".to_string(),
        cookie: "c".to_string(),
        user_id: None,
        divisor: None,
        original_filename: "stale.yaml".to_string(),
    });

    let effects = reduce(&mut session, AppAction::EnterAddNewApi);

    assert!(session.settings_ui.adding_newapi);
    assert!(session.settings_ui.editing_newapi.is_none()); // 确保进入纯新增模式
    assert!(has_render(&effects));
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
    assert!(session
        .settings
        .provider
        .is_quota_visible(ProviderKind::Claude, "session"));

    let effects = reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::ToggleQuotaVisibility {
            kind: ProviderKind::Claude,
            quota_key: "session".to_string(),
        }),
    );

    assert!(!session
        .settings
        .provider
        .is_quota_visible(ProviderKind::Claude, "session"));
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::PersistSettings))
    )));
    assert!(has_render(&effects));
}

#[test]
fn toggle_quota_visibility_round_trip() {
    let mut session = make_session();

    reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::ToggleQuotaVisibility {
            kind: ProviderKind::Claude,
            quota_key: "weekly".to_string(),
        }),
    );
    assert!(!session
        .settings
        .provider
        .is_quota_visible(ProviderKind::Claude, "weekly"));

    reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::ToggleQuotaVisibility {
            kind: ProviderKind::Claude,
            quota_key: "weekly".to_string(),
        }),
    );
    assert!(session
        .settings
        .provider
        .is_quota_visible(ProviderKind::Claude, "weekly"));
}

// ── ClearDebugLogs ──────────────────────────────────

#[test]
fn clear_debug_logs_produces_effect() {
    let mut session = make_session();
    let effects = reduce(&mut session, AppAction::ClearDebugLogs);

    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Debug(DebugEffect::ClearLogs))
    )));
    assert!(has_render(&effects));
}
// ── RefreshAll ──────────────────────────────────────

#[test]
fn refresh_all_marks_enabled_providers_refreshing() {
    let mut session = make_session();
    session
        .settings
        .provider
        .set_enabled(&pid(ProviderKind::Claude), true);
    session
        .settings
        .provider
        .set_enabled(&pid(ProviderKind::Gemini), true);

    let effects = reduce(&mut session, AppAction::RefreshAll);

    // 所有已启用的 provider 应被标记为 Refreshing
    let claude = session
        .provider_store
        .find_by_id(&pid(ProviderKind::Claude))
        .unwrap();
    assert_eq!(claude.connection, ConnectionStatus::Refreshing);
    let gemini = session
        .provider_store
        .find_by_id(&pid(ProviderKind::Gemini))
        .unwrap();
    assert_eq!(gemini.connection, ConnectionStatus::Refreshing);

    // 未启用的 provider 不受影响
    let copilot = session
        .provider_store
        .find_by_id(&pid(ProviderKind::Copilot))
        .unwrap();
    assert_ne!(copilot.connection, ConnectionStatus::Refreshing);

    // 应产出 RefreshAll 请求
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Refresh(RefreshEffect::SendRequest(
            RefreshRequest::RefreshAll { .. }
        )))
    )));
    assert!(has_render(&effects));
}

#[test]
fn refresh_all_with_no_enabled_providers_is_safe() {
    let mut session = make_session();
    // 默认没有启用任何 provider

    let effects = reduce(&mut session, AppAction::RefreshAll);

    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Refresh(RefreshEffect::SendRequest(
            RefreshRequest::RefreshAll { .. }
        )))
    )));
    assert!(!has_render(&effects));
}

#[test]
fn refresh_all_skips_non_monitorable_providers() {
    let mut session = make_session();
    let kilo_id = pid(ProviderKind::Kilo);
    session.settings.provider.set_enabled(&kilo_id, true);

    let effects = reduce(&mut session, AppAction::RefreshAll);

    let kilo = session.provider_store.find_by_id(&kilo_id).unwrap();
    assert_ne!(kilo.connection, ConnectionStatus::Refreshing);
    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Refresh(RefreshEffect::SendRequest(
            RefreshRequest::RefreshAll { .. }
        )))
    )));
    assert!(!has_render(&effects));
}

// ── RefreshEvent::Finished + debug restore ──────────

#[test]
fn finished_event_restores_debug_state() {
    let mut session = make_session();
    let id = pid(ProviderKind::Claude);

    session.debug_ui.selected_provider = Some(id.clone());
    session.debug_ui.refresh_active = true;
    session.debug_ui.prev_log_level = Some(log::LevelFilter::Info);

    let outcome = RefreshOutcome {
        id,
        result: RefreshResult::Failed {
            failure: crate::models::ProviderFailure {
                reason: crate::models::FailureReason::FetchFailed,
                advice: None,
                raw_detail: Some("test error".to_string()),
            },
            error_kind: crate::models::ErrorKind::NetworkError,
        },
    };
    let effects = reduce(
        &mut session,
        AppAction::RefreshEventReceived(RefreshEvent::Finished(outcome)),
    );

    assert!(!session.debug_ui.refresh_active);
    assert!(session.debug_ui.prev_log_level.is_none());
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Debug(DebugEffect::RestoreLogLevel(
            log::LevelFilter::Info
        )))
    )));
}

#[test]
fn finished_event_for_other_provider_does_not_restore() {
    let mut session = make_session();

    session.debug_ui.selected_provider = Some(pid(ProviderKind::Claude));
    session.debug_ui.refresh_active = true;
    session.debug_ui.prev_log_level = Some(log::LevelFilter::Info);

    let outcome = RefreshOutcome {
        id: pid(ProviderKind::Gemini),
        result: RefreshResult::SkippedCooldown,
    };
    let effects = reduce(
        &mut session,
        AppAction::RefreshEventReceived(RefreshEvent::Finished(outcome)),
    );

    assert!(session.debug_ui.refresh_active);
    assert!(session.debug_ui.prev_log_level.is_some());
    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Debug(DebugEffect::RestoreLogLevel(_)))
    )));
}

#[test]
fn finished_restore_survives_unknown_provider() {
    let mut session = make_session_without(ProviderKind::Claude);
    let id = pid(ProviderKind::Claude);

    session.debug_ui.selected_provider = Some(id.clone());
    session.debug_ui.refresh_active = true;
    session.debug_ui.prev_log_level = Some(log::LevelFilter::Warn);

    let outcome = RefreshOutcome {
        id,
        result: RefreshResult::Failed {
            failure: crate::models::ProviderFailure {
                reason: crate::models::FailureReason::FetchFailed,
                advice: None,
                raw_detail: Some("gone".to_string()),
            },
            error_kind: crate::models::ErrorKind::Unknown,
        },
    };
    let effects = reduce(
        &mut session,
        AppAction::RefreshEventReceived(RefreshEvent::Finished(outcome)),
    );

    assert!(!session.debug_ui.refresh_active);
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Debug(DebugEffect::RestoreLogLevel(
            log::LevelFilter::Warn
        )))
    )));
}

// ── ProvidersReloaded (热重载) ───────────────────────────

fn make_custom_provider_status(id: &str) -> crate::models::ProviderStatus {
    let provider_id = ProviderId::Custom(id.to_string());
    let metadata = crate::models::test_helpers::make_test_metadata(ProviderKind::Custom);
    crate::models::ProviderStatus::new(provider_id, metadata)
}

#[test]
fn providers_reloaded_sends_update_config() {
    let mut session = make_session();
    let statuses = session.provider_store.providers.to_vec();

    let effects = reduce(
        &mut session,
        AppAction::RefreshEventReceived(RefreshEvent::ProvidersReloaded { statuses }),
    );

    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Refresh(RefreshEffect::SendRequest(
            RefreshRequest::UpdateConfig { .. }
        )))
    )));
    assert!(has_render(&effects));
}

#[test]
fn providers_reloaded_refreshes_enabled_new_custom() {
    let mut session = make_session();
    let custom_id = ProviderId::Custom("new:api".to_string());
    session.settings.provider.set_enabled(&custom_id, true);

    let mut statuses = session.provider_store.providers.to_vec();
    statuses.push(make_custom_provider_status("new:api"));

    let effects = reduce(
        &mut session,
        AppAction::RefreshEventReceived(RefreshEvent::ProvidersReloaded { statuses }),
    );

    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Refresh(RefreshEffect::SendRequest(RefreshRequest::RefreshOne {
            ref id,
            ..
        }))) if *id == ProviderId::Custom("new:api".to_string())
    )));
}

#[test]
fn providers_reloaded_does_not_refresh_disabled_custom() {
    let mut session = make_session();

    // 明确禁用该 Provider（模拟用户手动关闭的场景）
    let custom_id = ProviderId::Custom("disabled:api".to_string());
    session.settings.provider.set_enabled(&custom_id, false);

    let mut statuses = session.provider_store.providers.to_vec();
    statuses.push(make_custom_provider_status("disabled:api"));

    let effects = reduce(
        &mut session,
        AppAction::RefreshEventReceived(RefreshEvent::ProvidersReloaded { statuses }),
    );

    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Refresh(RefreshEffect::SendRequest(RefreshRequest::RefreshOne {
            ref id,
            ..
        }))) if *id == ProviderId::Custom("disabled:api".to_string())
    )));
}

#[test]
fn providers_reloaded_clears_debug_selection_for_deleted_custom() {
    let mut session = make_session();
    let custom_id = ProviderId::Custom("old:api".to_string());
    session
        .provider_store
        .providers
        .push(make_custom_provider_status("old:api"));
    session.debug_ui.selected_provider = Some(custom_id);

    let statuses: Vec<_> = ProviderKind::all()
        .iter()
        .map(|k| make_test_provider(*k, ConnectionStatus::Disconnected))
        .collect();

    reduce(
        &mut session,
        AppAction::RefreshEventReceived(RefreshEvent::ProvidersReloaded { statuses }),
    );

    assert!(session.debug_ui.selected_provider.is_none());
}

#[test]
fn providers_reloaded_repoints_active_tab_when_deleted() {
    let mut session = make_session();
    let custom_id = ProviderId::Custom("gone:api".to_string());
    session
        .provider_store
        .providers
        .push(make_custom_provider_status("gone:api"));
    session.settings.provider.set_enabled(&custom_id, true);
    session.nav.switch_to(NavTab::Provider(custom_id.clone()));

    let statuses: Vec<_> = ProviderKind::all()
        .iter()
        .map(|k| make_test_provider(*k, ConnectionStatus::Disconnected))
        .collect();

    reduce(
        &mut session,
        AppAction::RefreshEventReceived(RefreshEvent::ProvidersReloaded { statuses }),
    );

    match &session.nav.active_tab {
        NavTab::Provider(id) => assert_ne!(*id, custom_id),
        NavTab::Settings | NavTab::Overview => {}
    }
}

#[test]
fn providers_reloaded_persists_settings_when_stale_ids_pruned() {
    let mut session = make_session();
    let custom_id = ProviderId::Custom("stale:api".to_string());
    session.settings.provider.set_enabled(&custom_id, true);
    session
        .provider_store
        .providers
        .push(make_custom_provider_status("stale:api"));

    let statuses: Vec<_> = ProviderKind::all()
        .iter()
        .map(|k| make_test_provider(*k, ConnectionStatus::Disconnected))
        .collect();

    let effects = reduce(
        &mut session,
        AppAction::RefreshEventReceived(RefreshEvent::ProvidersReloaded { statuses }),
    );

    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::PersistSettings))
    )));
}

// ── DeleteNewApi ──────────────────────────────────────────────────────────

#[test]
fn delete_newapi_produces_delete_effect_with_correct_provider_id() {
    let mut session = make_session();
    let id = ProviderId::Custom("my-api-example-com:newapi".to_string());
    session.settings_ui.confirming_delete_newapi = true;

    let effects = reduce(
        &mut session,
        AppAction::DeleteNewApi {
            provider_id: id.clone(),
        },
    );

    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::NewApi(NewApiEffect::DeleteProvider { provider_id }))
            if *provider_id == id
    )));
    assert!(!session.settings_ui.confirming_delete_newapi);
    assert!(has_render(&effects));
}

#[test]
fn delete_newapi_emits_effect_for_any_provider_id() {
    // 文件名推导和 `:newapi` 检查已移至 runtime，reducer 统一发射
    let mut session = make_session();
    let id = ProviderId::Custom("some-other-provider:cli".to_string());

    let effects = reduce(
        &mut session,
        AppAction::DeleteNewApi {
            provider_id: id.clone(),
        },
    );

    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::NewApi(NewApiEffect::DeleteProvider { provider_id }))
            if *provider_id == id
    )));
}

#[test]
fn delete_newapi_emits_effect_for_builtin_provider() {
    let mut session = make_session();
    let id = ProviderId::BuiltIn(ProviderKind::Claude);

    let effects = reduce(&mut session, AppAction::DeleteNewApi { provider_id: id });

    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::NewApi(NewApiEffect::DeleteProvider { .. }))
    )));
}

// ── MoveProviderToIndex（拖拽排序）──────────────────

#[test]
fn move_provider_to_index_persists_and_renders() {
    let mut session = make_session();
    // Claude 默认在 index 0，移动到末尾以确保触发状态变更
    let total = ProviderKind::all().len();
    let effects = reduce(
        &mut session,
        AppAction::MoveProviderToIndex {
            id: pid(ProviderKind::Claude),
            target_index: total - 1,
        },
    );

    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::PersistSettings))
    )));
    assert!(has_render(&effects));
}

#[test]
fn move_provider_to_same_index_produces_no_effects() {
    let mut session = make_session();
    // 首先获取 claude 的当前位置
    let custom_ids = session.provider_store.custom_provider_ids();
    let ordered = session.settings.provider.ordered_provider_ids(&custom_ids);
    let claude_index = ordered
        .iter()
        .position(|id| *id == pid(ProviderKind::Claude))
        .unwrap();

    let effects = reduce(
        &mut session,
        AppAction::MoveProviderToIndex {
            id: pid(ProviderKind::Claude),
            target_index: claude_index,
        },
    );

    assert!(effects.is_empty());
}

// ── Sidebar dynamic list ────────────────────────────

#[test]
fn enter_add_provider_sets_flag_and_clears_newapi() {
    let mut session = make_session();
    session.settings_ui.adding_newapi = true;

    let effects = reduce(&mut session, AppAction::EnterAddProvider);

    assert!(session.settings_ui.adding_provider);
    assert!(!session.settings_ui.adding_newapi); // 互斥
    assert!(has_render(&effects));
}

#[test]
fn cancel_add_provider_clears_flag() {
    let mut session = make_session();
    session.settings_ui.adding_provider = true;

    let effects = reduce(&mut session, AppAction::CancelAddProvider);

    assert!(!session.settings_ui.adding_provider);
    assert!(has_render(&effects));
}

#[test]
fn add_provider_to_sidebar_persists_and_selects() {
    let mut session = make_session();
    // 预设 sidebar 只有 claude
    session.settings.provider.sidebar_providers = vec!["claude".into()];
    session.settings_ui.adding_provider = true;

    let id = pid(ProviderKind::Gemini);
    let effects = reduce(&mut session, AppAction::AddProviderToSidebar(id.clone()));

    // sidebar 现在包含 gemini
    assert!(session
        .settings
        .provider
        .sidebar_providers
        .contains(&"gemini".to_string()));
    // 选中了刚添加的 provider
    assert_eq!(session.settings_ui.selected_provider, id);
    // 退出添加模式
    assert!(!session.settings_ui.adding_provider);
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::PersistSettings))
    )));
    assert!(has_render(&effects));
}

#[test]
fn remove_provider_from_sidebar_disables_and_persists() {
    let mut session = make_session();
    session.settings.provider.sidebar_providers = vec!["claude".into(), "gemini".into()];
    session
        .settings
        .provider
        .set_enabled(&pid(ProviderKind::Claude), true);
    session
        .settings
        .provider
        .set_enabled(&pid(ProviderKind::Gemini), true);

    let effects = reduce(
        &mut session,
        AppAction::RemoveProviderFromSidebar(pid(ProviderKind::Claude)),
    );

    // claude 不在 sidebar 中了
    assert!(!session
        .settings
        .provider
        .sidebar_providers
        .contains(&"claude".to_string()));
    // claude 被 disable
    assert!(!session
        .settings
        .provider
        .is_enabled(&pid(ProviderKind::Claude)));
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::PersistSettings))
    )));
    assert!(has_render(&effects));
}

#[test]
fn remove_last_sidebar_provider_enters_add_mode() {
    let mut session = make_session();
    session.settings.provider.sidebar_providers = vec!["claude".into()];
    session
        .settings
        .provider
        .set_enabled(&pid(ProviderKind::Claude), true);

    let effects = reduce(
        &mut session,
        AppAction::RemoveProviderFromSidebar(pid(ProviderKind::Claude)),
    );

    // sidebar 已空
    assert!(session.settings.provider.sidebar_providers.is_empty());
    // 自动进入添加模式
    assert!(session.settings_ui.adding_provider);
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::PersistSettings))
    )));
    assert!(has_render(&effects));
}

#[test]
fn remove_nonexistent_provider_from_sidebar_is_noop() {
    let mut session = make_session();
    session.settings.provider.sidebar_providers = vec!["claude".into()];

    let effects = reduce(
        &mut session,
        AppAction::RemoveProviderFromSidebar(pid(ProviderKind::Gemini)),
    );

    // sidebar 不变
    assert_eq!(session.settings.provider.sidebar_providers.len(), 1);
    // 无持久化
    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::PersistSettings))
    )));
    // 仍有 render（Render effect 在 if 外无条件 push）
    assert!(has_render(&effects));
}

// ── 二次确认状态 ──────────────────────────────────────

#[test]
fn confirm_remove_provider_sets_confirming_flag() {
    let mut session = make_session();
    assert!(!session.settings_ui.confirming_remove_provider);

    let effects = reduce(&mut session, AppAction::ConfirmRemoveProvider);

    assert!(session.settings_ui.confirming_remove_provider);
    assert!(has_render(&effects));
}

#[test]
fn cancel_remove_provider_clears_confirming_flag() {
    let mut session = make_session();
    session.settings_ui.confirming_remove_provider = true;

    let effects = reduce(&mut session, AppAction::CancelRemoveProvider);

    assert!(!session.settings_ui.confirming_remove_provider);
    assert!(has_render(&effects));
}

#[test]
fn remove_provider_resets_confirming_flag() {
    let mut session = make_session();
    session.settings.provider.sidebar_providers = vec!["claude".into(), "gemini".into()];
    session.settings_ui.confirming_remove_provider = true;

    reduce(
        &mut session,
        AppAction::RemoveProviderFromSidebar(pid(ProviderKind::Claude)),
    );

    assert!(!session.settings_ui.confirming_remove_provider);
}

#[test]
fn confirm_delete_newapi_sets_confirming_flag() {
    let mut session = make_session();
    assert!(!session.settings_ui.confirming_delete_newapi);

    let effects = reduce(&mut session, AppAction::ConfirmDeleteNewApi);

    assert!(session.settings_ui.confirming_delete_newapi);
    assert!(has_render(&effects));
}

#[test]
fn cancel_delete_newapi_clears_confirming_flag() {
    let mut session = make_session();
    session.settings_ui.confirming_delete_newapi = true;

    let effects = reduce(&mut session, AppAction::CancelDeleteNewApi);

    assert!(!session.settings_ui.confirming_delete_newapi);
    assert!(has_render(&effects));
}

#[test]
fn select_provider_resets_confirming_flags() {
    let mut session = make_session();
    session.settings_ui.confirming_remove_provider = true;
    session.settings_ui.confirming_delete_newapi = true;
    session.settings_ui.token_editing_provider = Some(pid(ProviderKind::Copilot));

    reduce(
        &mut session,
        AppAction::SelectSettingsProvider(pid(ProviderKind::Gemini)),
    );

    assert!(!session.settings_ui.confirming_remove_provider);
    assert!(!session.settings_ui.confirming_delete_newapi);
    assert!(session.settings_ui.token_editing_provider.is_none());
}

#[test]
fn enter_add_newapi_clears_adding_provider() {
    let mut session = make_session();
    session.settings_ui.adding_provider = true;

    let effects = reduce(&mut session, AppAction::EnterAddNewApi);

    assert!(session.settings_ui.adding_newapi);
    assert!(!session.settings_ui.adding_provider); // 互斥清除
    assert!(has_render(&effects));
}

// ── Token Editing / Saving ────────────────────────────

#[test]
fn set_token_editing_enables_editing() {
    let mut session = make_session();
    assert!(session.settings_ui.token_editing_provider.is_none());

    let effects = reduce(
        &mut session,
        AppAction::SetTokenEditing {
            provider_id: pid(ProviderKind::Copilot),
            editing: true,
        },
    );

    assert_eq!(
        session.settings_ui.token_editing_provider,
        Some(pid(ProviderKind::Copilot))
    );
    assert!(has_render(&effects));
}

#[test]
fn set_token_editing_disables_editing() {
    let mut session = make_session();
    session.settings_ui.token_editing_provider = Some(pid(ProviderKind::Copilot));

    let effects = reduce(
        &mut session,
        AppAction::SetTokenEditing {
            provider_id: pid(ProviderKind::Copilot),
            editing: false,
        },
    );

    assert!(session.settings_ui.token_editing_provider.is_none());
    assert!(has_render(&effects));
}

#[test]
fn save_provider_token_stores_credential_and_persists() {
    let mut session = make_session();
    session.settings_ui.token_editing_provider = Some(pid(ProviderKind::Copilot));

    let effects = reduce(
        &mut session,
        AppAction::SaveProviderToken {
            provider_id: pid(ProviderKind::Copilot),
            token: "ghp_test123".to_string(),
        },
    );

    // token 已存储
    assert_eq!(
        session
            .settings
            .provider
            .credentials
            .get_credential("github_token"),
        Some("ghp_test123")
    );
    // 编辑状态已关闭
    assert!(session.settings_ui.token_editing_provider.is_none());
    // 产出 PersistSettings
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::PersistSettings))
    )));
    assert!(has_render(&effects));
}

#[test]
fn save_provider_token_empty_does_not_persist() {
    let mut session = make_session();
    session.settings_ui.token_editing_provider = Some(pid(ProviderKind::Copilot));

    let effects = reduce(
        &mut session,
        AppAction::SaveProviderToken {
            provider_id: pid(ProviderKind::Copilot),
            token: "   ".to_string(), // 空白
        },
    );

    // 不应存储
    assert!(session
        .settings
        .provider
        .credentials
        .get_credential("github_token")
        .is_none());
    // 编辑状态仍关闭
    assert!(session.settings_ui.token_editing_provider.is_none());
    // 不应产出 PersistSettings
    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::PersistSettings))
    )));
}

#[test]
fn save_provider_token_without_capability_does_not_persist() {
    let mut session = make_session();
    // Claude 没有 TokenInput capability
    session.settings_ui.token_editing_provider = Some(pid(ProviderKind::Claude));

    let effects = reduce(
        &mut session,
        AppAction::SaveProviderToken {
            provider_id: pid(ProviderKind::Claude),
            token: "some_token".to_string(),
        },
    );

    // 不应产出 PersistSettings（capability 不匹配）
    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::PersistSettings))
    )));
    // 编辑状态仍关闭
    assert!(session.settings_ui.token_editing_provider.is_none());
}

#[test]
fn save_provider_token_supports_arbitrary_credential_key() {
    let custom_id = ProviderId::Custom("custom-token:api".to_string());
    let mut session = make_session();
    session
        .provider_store
        .providers
        .push(make_custom_token_provider(
            "custom-token:api",
            "custom_token",
        ));
    session.settings_ui.token_editing_provider = Some(custom_id.clone());

    let effects = reduce(
        &mut session,
        AppAction::SaveProviderToken {
            provider_id: custom_id,
            token: "custom-secret".to_string(),
        },
    );

    assert_eq!(
        session
            .settings
            .provider
            .credentials
            .get_credential("custom_token"),
        Some("custom-secret")
    );
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::PersistSettings))
    )));
}
