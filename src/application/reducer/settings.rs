use crate::application::{
    AppEffect, ContextEffect, NotificationEffect, RefreshEffect, SettingChange, SettingsEffect,
    TrayIconRequest,
};
use crate::models::{NavTab, ProviderId, TrayIconStyle};

use super::super::state::{AppSession, SettingsModalState, SettingsTab};
use super::shared::{build_config_sync_request, resolve_tray_icon_request};

pub(super) fn select_nav_tab(session: &mut AppSession, tab: NavTab, effects: &mut Vec<AppEffect>) {
    session.nav.switch_to(tab);
    effects.push(ContextEffect::Render.into());
}

pub(super) fn set_settings_tab(
    session: &mut AppSession,
    tab: SettingsTab,
    effects: &mut Vec<AppEffect>,
) {
    session.settings_ui.active_tab = tab;
    session.settings_ui.token_editing_provider = None;
    // 切换 tab 时退出添加内置服务商的 picker（轻量操作，点走即退）。
    // NewAPI 表单和二次确认态保留，回到 Providers tab 仍能继续。
    if session.settings_ui.modal.is_adding_provider() {
        session.settings_ui.modal = SettingsModalState::Idle;
    }
    effects.push(ContextEffect::Render.into());
}

pub(super) fn toggle_cadence_dropdown(session: &mut AppSession, effects: &mut Vec<AppEffect>) {
    session.settings_ui.cadence_dropdown_open = !session.settings_ui.cadence_dropdown_open;
    effects.push(ContextEffect::Render.into());
}

pub(super) fn save_global_hotkey(
    session: &mut AppSession,
    hotkey: String,
    effects: &mut Vec<AppEffect>,
) {
    session.settings_ui.clear_global_hotkey_error();
    effects.push(ContextEffect::ApplyGlobalHotkey(hotkey).into());
    effects.push(ContextEffect::Render.into());
}

pub(super) fn apply_setting_change(
    session: &mut AppSession,
    change: SettingChange,
    effects: &mut Vec<AppEffect>,
) {
    match change {
        SettingChange::ToggleAutoHideWindow => {
            session.settings.system.auto_hide_window = !session.settings.system.auto_hide_window;
        }
        SettingChange::ToggleStartAtLogin => {
            let new_val = !session.settings.system.start_at_login;
            session.settings.system.start_at_login = new_val;
            effects.push(SettingsEffect::SyncAutoLaunch(new_val).into());
            effects.push(NotificationEffect::AutoLaunchToggled { enabled: new_val }.into());
        }
        SettingChange::ToggleSessionQuotaNotifications => {
            session.settings.notification.session_quota_notifications =
                !session.settings.notification.session_quota_notifications;
        }
        SettingChange::ToggleNotificationSound => {
            session.settings.notification.notification_sound =
                !session.settings.notification.notification_sound;
        }
        SettingChange::ToggleShowDashboardButton => {
            session.settings.display.show_dashboard_button =
                !session.settings.display.show_dashboard_button;
        }
        SettingChange::ToggleShowRefreshButton => {
            session.settings.display.show_refresh_button =
                !session.settings.display.show_refresh_button;
        }
        SettingChange::ToggleShowDebugTab => {
            let new_val = !session.settings.display.show_debug_tab;
            session.settings.display.show_debug_tab = new_val;
            if !new_val && session.settings_ui.active_tab == SettingsTab::Debug {
                session.settings_ui.active_tab = SettingsTab::General;
            }
        }
        SettingChange::ToggleShowAccountInfo => {
            session.settings.display.show_account_info =
                !session.settings.display.show_account_info;
        }
        SettingChange::ToggleShowOverview => {
            let new_val = !session.settings.display.show_overview;
            session.settings.display.show_overview = new_val;
            // 关闭 Overview 时，如果当前在 Overview tab，则切换到第一个 Provider
            if !new_val && session.nav.active_tab == NavTab::Overview {
                if let Some(tab) = session.default_provider_tab() {
                    session.nav.switch_to(tab);
                }
            }
        }
        SettingChange::Theme(theme) => {
            session.settings.display.theme = theme;
        }
        SettingChange::Language(language) => {
            session.settings.display.language = language.clone();
            effects.push(SettingsEffect::ApplyLocale(language).into());
        }
        SettingChange::RefreshCadence(mins) => {
            session.settings.system.refresh_interval_mins = mins.unwrap_or(0);
            session.settings_ui.cadence_dropdown_open = false;
            effects.push(RefreshEffect::SendRequest(build_config_sync_request(session)).into());
        }
        SettingChange::SetTrayIconStyle(style) => {
            session.settings.display.tray_icon_style = style;
            effects.push(
                ContextEffect::ApplyTrayIcon(resolve_tray_icon_request(session, style)).into(),
            );
        }
        SettingChange::SetQuotaDisplayMode(mode) => {
            session.settings.display.quota_display_mode = mode;
        }
        SettingChange::ToggleQuotaVisibility {
            provider_id,
            quota_key,
        } => {
            session
                .settings
                .provider
                .toggle_quota_visibility(&provider_id, quota_key);
        }
    }

    effects.push(SettingsEffect::PersistSettings.into());
    effects.push(ContextEffect::Render.into());
}

pub(super) fn open_settings(
    session: &mut AppSession,
    provider: Option<ProviderId>,
    effects: &mut Vec<AppEffect>,
) {
    if let Some(id) = provider {
        session.settings_ui.selected_provider = id;
        session.settings_ui.active_tab = SettingsTab::Providers;
    }
    effects.push(ContextEffect::OpenSettingsWindow.into());
}

pub(super) fn open_url(url: String, effects: &mut Vec<AppEffect>) {
    effects.push(ContextEffect::OpenUrl(url).into());
}

pub(super) fn popup_visibility_changed(
    session: &mut AppSession,
    visible: bool,
    effects: &mut Vec<AppEffect>,
) {
    session.popup_visible = visible;
    if !visible {
        // 弹窗关闭时同步图标为已启用 Provider 的综合状态
        if session.settings.display.tray_icon_style == TrayIconStyle::Dynamic {
            effects.push(
                ContextEffect::ApplyTrayIcon(TrayIconRequest::DynamicStatus(
                    session.worst_enabled_provider_status(),
                ))
                .into(),
            );
        }
    }
}

pub(super) fn quit_app(effects: &mut Vec<AppEffect>) {
    effects.push(ContextEffect::QuitApp.into());
}
