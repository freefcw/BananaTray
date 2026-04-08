use crate::app_state::{AppSession, SettingsTab};
use crate::application::{
    AppAction, AppEffect, ProviderOrderDirection, SettingChange, TrayIconRequest,
};
use crate::models::{NavTab, ProviderId, StatusLevel, TrayIconStyle};
use crate::refresh::{RefreshEvent, RefreshReason, RefreshRequest, RefreshResult};
use log::{debug, info};

pub fn reduce(session: &mut AppSession, action: AppAction) -> Vec<AppEffect> {
    let mut effects = Vec::new();

    match action {
        AppAction::SelectNavTab(tab) => {
            session.nav.switch_to(tab);
            push_render(&mut effects);
        }
        AppAction::SetSettingsTab(tab) => {
            session.settings_ui.active_tab = tab;
            push_render(&mut effects);
        }
        AppAction::SelectSettingsProvider(id) => {
            session.settings_ui.selected_provider = id;
            session.settings_ui.adding_newapi = false;
            push_render(&mut effects);
        }
        AppAction::ToggleCadenceDropdown => {
            session.settings_ui.cadence_dropdown_open = !session.settings_ui.cadence_dropdown_open;
            push_render(&mut effects);
        }
        AppAction::SetCopilotTokenEditing(editing) => {
            session.settings_ui.copilot_token_editing = editing;
            push_render(&mut effects);
        }
        AppAction::SaveCopilotToken(token) => {
            let token = token.trim().to_string();
            if !token.is_empty() {
                session.settings.provider.credentials.github_token = Some(token);
                effects.push(AppEffect::PersistSettings);
            }
            session.settings_ui.copilot_token_editing = false;
            push_render(&mut effects);
        }
        AppAction::ReorderProvider { id, direction } => {
            let custom_ids = session.provider_store.custom_provider_ids();
            let moved = match direction {
                ProviderOrderDirection::Up => session.settings.move_provider_up(&id, &custom_ids),
                ProviderOrderDirection::Down => {
                    session.settings.move_provider_down(&id, &custom_ids)
                }
            };
            if moved {
                effects.push(AppEffect::PersistSettings);
                push_render(&mut effects);
            }
        }
        AppAction::UpdateSetting(change) => {
            apply_setting_change(session, change, &mut effects);
        }
        AppAction::RefreshProvider { id, reason } => {
            request_provider_refresh(session, id, reason, &mut effects);
        }
        AppAction::ToggleProvider(id) => {
            toggle_provider(session, id, &mut effects);
        }
        AppAction::RefreshEventReceived(event) => {
            apply_refresh_event(session, event, &mut effects);
        }
        AppAction::OpenSettings { provider } => {
            if let Some(id) = provider {
                session.settings_ui.selected_provider = id;
                session.settings_ui.active_tab = SettingsTab::Providers;
            }
            effects.push(AppEffect::OpenSettingsWindow);
        }
        AppAction::OpenDashboard(id) => {
            if let Some(provider) = session.provider_store.find_by_id(&id) {
                let url = provider.dashboard_url().trim();
                if !url.is_empty() {
                    effects.push(AppEffect::OpenUrl(url.to_string()));
                }
            }
        }
        AppAction::OpenUrl(url) => effects.push(AppEffect::OpenUrl(url)),
        AppAction::UpdateLogLevel(level) => {
            effects.push(AppEffect::UpdateLogLevel(level));
            push_render(&mut effects);
        }
        AppAction::SendDebugNotification(kind) => {
            effects.push(AppEffect::SendDebugNotification {
                kind,
                with_sound: session.settings.notification.notification_sound,
            });
        }
        AppAction::OpenLogDirectory => {
            effects.push(AppEffect::OpenLogDirectory);
        }
        AppAction::CopyToClipboard(text) => {
            effects.push(AppEffect::CopyToClipboard(text));
        }
        AppAction::SelectDebugProvider(id) => {
            session.debug_ui.selected_provider = Some(id);
            push_render(&mut effects);
        }
        AppAction::DebugRefreshProvider => {
            if let Some(ref id) = session.debug_ui.selected_provider {
                if !session.debug_ui.refresh_active {
                    session.debug_ui.refresh_active = true;
                    session.provider_store.mark_refreshing_by_id(id);
                    effects.push(AppEffect::StartDebugRefresh(id.clone()));
                    push_render(&mut effects);
                }
            }
        }
        AppAction::ClearDebugLogs => {
            effects.push(AppEffect::ClearDebugLogs);
            push_render(&mut effects);
        }
        AppAction::PopupVisibilityChanged(visible) => {
            session.popup_visible = visible;
            if !visible {
                // 弹窗关闭时同步图标为当前 Provider 的状态
                if session.settings.display.tray_icon_style == TrayIconStyle::Dynamic {
                    effects.push(AppEffect::ApplyTrayIcon(TrayIconRequest::DynamicStatus(
                        session.current_provider_status(),
                    )));
                }
            }
        }
        AppAction::EnterAddNewApi => {
            session.settings_ui.adding_newapi = true;
            session.settings_ui.editing_newapi = None; // 确保进入纯新增模式
            push_render(&mut effects);
        }
        AppAction::CancelAddNewApi => {
            session.settings_ui.adding_newapi = false;
            session.settings_ui.editing_newapi = None;
            push_render(&mut effects);
        }
        AppAction::SubmitNewApi {
            display_name,
            base_url,
            cookie,
            user_id,
            divisor,
        } => {
            use crate::providers::custom::generator;

            let is_editing = session.settings_ui.editing_newapi.is_some();
            let original_filename = session
                .settings_ui
                .editing_newapi
                .as_ref()
                .map(|d| d.original_filename.clone());

            let config = generator::NewApiConfig {
                display_name,
                base_url,
                cookie,
                user_id,
                divisor,
            };
            let yaml_content = generator::generate_newapi_yaml(&config);
            // 编辑模式：沿用原文件名（身份不变）；新增模式：根据 URL 生成
            let filename =
                original_filename.unwrap_or_else(|| generator::generate_filename(&config));

            effects.push(AppEffect::SaveCustomProviderYaml {
                yaml_content,
                filename,
            });

            let (title_key, body_key) = if is_editing {
                ("newapi.edit_success_title", "newapi.edit_success_body")
            } else {
                ("newapi.save_success_title", "newapi.save_success_body")
            };
            effects.push(AppEffect::SendPlainNotification {
                title: rust_i18n::t!(title_key).to_string(),
                body: rust_i18n::t!(body_key).to_string(),
            });
            session.settings_ui.adding_newapi = false;
            session.settings_ui.editing_newapi = None;
            push_render(&mut effects);
        }
        AppAction::EditNewApi { provider_id } => {
            use crate::providers::custom::generator;

            if let ProviderId::Custom(ref custom_id) = provider_id {
                if let Some(edit_data) = generator::read_newapi_config(custom_id) {
                    session.settings_ui.adding_newapi = true;
                    session.settings_ui.editing_newapi = Some(edit_data);
                    push_render(&mut effects);
                } else {
                    log::warn!(
                        target: "settings",
                        "EditNewApi: failed to read config for {}",
                        custom_id
                    );
                }
            }
        }
        AppAction::QuitApp => effects.push(AppEffect::QuitApp),
    }

    effects
}

fn apply_setting_change(
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
            effects.push(AppEffect::SyncAutoLaunch(new_val));
            // 自启动状态变更通知（与 SyncAutoLaunch 解耦，各自单一职责）
            let (title, body) = if new_val {
                (
                    rust_i18n::t!("notification.auto_launch.enabled.title").to_string(),
                    rust_i18n::t!("notification.auto_launch.enabled.body").to_string(),
                )
            } else {
                (
                    rust_i18n::t!("notification.auto_launch.disabled.title").to_string(),
                    rust_i18n::t!("notification.auto_launch.disabled.body").to_string(),
                )
            };
            effects.push(AppEffect::SendPlainNotification { title, body });
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
        SettingChange::Theme(theme) => {
            session.settings.display.theme = theme;
        }
        SettingChange::Language(language) => {
            session.settings.display.language = language.clone();
            effects.push(AppEffect::ApplyLocale(language));
        }
        SettingChange::RefreshCadence(mins) => {
            session.settings.system.refresh_interval_mins = mins.unwrap_or(0);
            session.settings_ui.cadence_dropdown_open = false;
            effects.push(AppEffect::SendRefreshRequest(build_config_sync_request(
                session,
            )));
        }
        SettingChange::SetTrayIconStyle(style) => {
            session.settings.display.tray_icon_style = style;
            effects.push(AppEffect::ApplyTrayIcon(resolve_tray_icon_request(
                session, style,
            )));
        }
        SettingChange::SetQuotaDisplayMode(mode) => {
            session.settings.display.quota_display_mode = mode;
        }
        SettingChange::ToggleQuotaVisibility { kind, quota_key } => {
            session.settings.toggle_quota_visibility(kind, quota_key);
        }
    }

    effects.push(AppEffect::PersistSettings);
    push_render(effects);
}

fn request_provider_refresh(
    session: &mut AppSession,
    id: ProviderId,
    reason: RefreshReason,
    effects: &mut Vec<AppEffect>,
) {
    if !session.settings.is_enabled(&id) {
        debug!(
            target: "refresh",
            "ignoring refresh request for disabled provider {}",
            id
        );
        return;
    }

    session.provider_store.mark_refreshing_by_id(&id);
    effects.push(AppEffect::SendRefreshRequest(RefreshRequest::RefreshOne {
        id,
        reason,
    }));
    push_render(effects);
}

fn toggle_provider(session: &mut AppSession, id: ProviderId, effects: &mut Vec<AppEffect>) {
    let new_val = !session.settings.is_enabled(&id);
    info!(
        target: "providers",
        "toggling provider {} from {} to {}",
        id,
        !new_val,
        new_val
    );
    session.settings.set_enabled(&id, new_val);

    if new_val {
        session.nav.switch_to(NavTab::Provider(id.clone()));
    } else {
        let providers = &session.provider_store.providers;
        session
            .nav
            .fallback_on_disable(&id, providers, &session.settings);
    }

    effects.push(AppEffect::PersistSettings);
    effects.push(AppEffect::SendRefreshRequest(build_config_sync_request(
        session,
    )));
    if new_val {
        request_provider_refresh(session, id, RefreshReason::ProviderToggled, effects);
    } else {
        // Provider 被禁用后需重新计算动态图标
        if session.settings.display.tray_icon_style == TrayIconStyle::Dynamic
            && !session.popup_visible
        {
            let status = session.current_provider_status();
            effects.push(AppEffect::ApplyTrayIcon(TrayIconRequest::DynamicStatus(
                status,
            )));
        }
        push_render(effects);
    }
}

fn apply_refresh_event(
    session: &mut AppSession,
    event: RefreshEvent,
    effects: &mut Vec<AppEffect>,
) {
    match event {
        RefreshEvent::Started { id } => {
            session.provider_store.mark_refreshing_by_id(&id);
            push_render(effects);
        }
        RefreshEvent::Finished(outcome) => {
            let is_debug_target = session.debug_ui.refresh_active
                && session.debug_ui.selected_provider.as_ref() == Some(&outcome.id);

            // 快照刷新前的状态等级，用于判断刷新后是否需要更新图标
            let prev_status = session.current_provider_status();

            'process: {
                if session.provider_store.find_by_id(&outcome.id).is_none() {
                    break 'process;
                }

                match outcome.result {
                    RefreshResult::Success { data } => {
                        info!(
                            target: "providers",
                            "provider {} refresh succeeded: {} quotas",
                            outcome.id,
                            data.quotas.len()
                        );
                        let provider_name = session
                            .provider_store
                            .find_by_id(&outcome.id)
                            .map(|provider| provider.display_name().to_string())
                            .unwrap_or_else(|| format!("{}", outcome.id));
                        if let Some(alert) =
                            session
                                .alert_tracker
                                .update(&outcome.id, &provider_name, &data.quotas)
                        {
                            if session.settings.notification.session_quota_notifications {
                                effects.push(AppEffect::SendQuotaNotification {
                                    alert,
                                    with_sound: session.settings.notification.notification_sound,
                                });
                            }
                        }
                        let Some(provider) = session.provider_store.find_by_id_mut(&outcome.id)
                        else {
                            break 'process;
                        };
                        provider.mark_refresh_succeeded(data);
                        push_render(effects);
                    }
                    RefreshResult::Unavailable { message } => {
                        debug!(
                            target: "providers",
                            "provider {} unavailable: {}",
                            outcome.id,
                            message
                        );
                        let Some(provider) = session.provider_store.find_by_id_mut(&outcome.id)
                        else {
                            break 'process;
                        };
                        provider.mark_unavailable(message);
                        push_render(effects);
                    }
                    RefreshResult::Failed { error, error_kind } => {
                        let Some(provider) = session.provider_store.find_by_id_mut(&outcome.id)
                        else {
                            break 'process;
                        };
                        provider.mark_refresh_failed(error, error_kind);
                        push_render(effects);
                    }
                    RefreshResult::SkippedCooldown
                    | RefreshResult::SkippedInFlight
                    | RefreshResult::SkippedDisabled => {}
                }
            }

            // 动态图标：仅当刷新的是当前 Provider 时才检查状态变化
            maybe_update_dynamic_icon(session, &outcome.id, prev_status, effects);

            if is_debug_target {
                session.debug_ui.refresh_active = false;
                if let Some(prev_level) = session.debug_ui.prev_log_level.take() {
                    effects.push(AppEffect::RestoreLogLevel(prev_level));
                }
                push_render(effects);
            }
        }
        RefreshEvent::ProvidersReloaded { statuses } => {
            info!(target: "providers", "providers reloaded: {} statuses", statuses.len());

            let affected = session.provider_store.sync_custom_providers(&statuses);

            // 清理 settings 中残留的已删除自定义 Provider ID
            let custom_ids = session.provider_store.custom_provider_ids();
            if session
                .settings
                .provider
                .prune_stale_custom_ids(&custom_ids)
            {
                effects.push(AppEffect::PersistSettings);
            }

            // 清理可能指向已删除 provider 的导航/设置引用
            sanitize_stale_refs(session);

            // 同步 coordinator 的 enabled 列表
            effects.push(AppEffect::SendRefreshRequest(build_config_sync_request(
                session,
            )));

            // 对新增/更新的自定义 Provider 立即触发刷新
            for id in &affected {
                if session.settings.is_enabled(id) {
                    session.provider_store.mark_refreshing_by_id(id);
                    effects.push(AppEffect::SendRefreshRequest(RefreshRequest::RefreshOne {
                        id: id.clone(),
                        reason: RefreshReason::ProviderToggled,
                    }));
                }
            }

            push_render(effects);
        }
    }
}

pub fn build_config_sync_request(session: &AppSession) -> RefreshRequest {
    let enabled: Vec<ProviderId> = session
        .provider_store
        .providers
        .iter()
        .filter(|p| session.settings.is_enabled(&p.provider_id))
        .map(|p| p.provider_id.clone())
        .collect();

    RefreshRequest::UpdateConfig {
        interval_mins: session.settings.system.refresh_interval_mins,
        enabled,
    }
}

/// 热重载后清理指向已删除 Provider 的引用
fn sanitize_stale_refs(session: &mut AppSession) {
    // 导航：如果当前 active_tab 指向的 provider 已不存在，回退
    if let NavTab::Provider(ref id) = session.nav.active_tab {
        if session.provider_store.find_by_id(id).is_none() {
            if let Some(tab) = session.default_provider_tab() {
                session.nav.switch_to(tab);
            } else {
                session.nav.switch_to(NavTab::Settings);
            }
        }
    }
    // last_provider_id
    if session
        .provider_store
        .find_by_id(&session.nav.last_provider_id)
        .is_none()
    {
        if let Some(first) = session
            .provider_store
            .providers
            .iter()
            .find(|p| session.settings.is_enabled(&p.provider_id))
        {
            session.nav.last_provider_id = first.provider_id.clone();
        }
    }
    // 设置面板选中的 provider
    if session
        .provider_store
        .find_by_id(&session.settings_ui.selected_provider)
        .is_none()
    {
        session.settings_ui.selected_provider =
            ProviderId::BuiltIn(crate::models::ProviderKind::Claude);
    }
    // Debug 面板
    if let Some(ref id) = session.debug_ui.selected_provider {
        if session.provider_store.find_by_id(id).is_none() {
            session.debug_ui.selected_provider = None;
        }
    }
}

fn push_render(effects: &mut Vec<AppEffect>) {
    effects.push(AppEffect::Render);
}

/// 将用户选择的 TrayIconStyle 解析为具体的 TrayIconRequest。
/// Dynamic 模式时根据当前 Provider 状态计算颜色，其余模式直接映射为静态请求。
fn resolve_tray_icon_request(session: &AppSession, style: TrayIconStyle) -> TrayIconRequest {
    if style == TrayIconStyle::Dynamic {
        TrayIconRequest::DynamicStatus(session.current_provider_status())
    } else {
        TrayIconRequest::Static(style)
    }
}

/// 若处于 Dynamic 模式，且刷新的是当前 Provider，且弹窗不可见，且状态发生变化时，
/// 追加 ApplyTrayIcon effect。
fn maybe_update_dynamic_icon(
    session: &AppSession,
    refreshed_id: &ProviderId,
    prev_status: StatusLevel,
    effects: &mut Vec<AppEffect>,
) {
    if session.settings.display.tray_icon_style != TrayIconStyle::Dynamic {
        return;
    }
    // 弹窗可见时延迟更新，关闭时由 PopupVisibilityChanged(false) 同步
    if session.popup_visible {
        return;
    }
    // 只响应当前 Provider 的刷新事件
    if *refreshed_id != session.nav.last_provider_id {
        return;
    }
    let new_status = session.current_provider_status();
    if new_status != prev_status {
        effects.push(AppEffect::ApplyTrayIcon(TrayIconRequest::DynamicStatus(
            new_status,
        )));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::test_helpers::make_test_provider;
    use crate::models::{AppSettings, ConnectionStatus, ProviderId, ProviderKind, RefreshData};
    use crate::refresh::{RefreshEvent, RefreshOutcome, RefreshResult};

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

    fn has_effect(effects: &[AppEffect], f: impl Fn(&AppEffect) -> bool) -> bool {
        effects.iter().any(f)
    }

    fn has_render(effects: &[AppEffect]) -> bool {
        has_effect(effects, |e| matches!(e, AppEffect::Render))
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
            AppEffect::PersistSettings
        )));
        assert!(has_effect(&effects, |e| matches!(
            e,
            AppEffect::SendRefreshRequest(_)
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
            AppEffect::SyncAutoLaunch(true)
        )));
        assert!(has_effect(&effects, |e| matches!(
            e,
            AppEffect::SendPlainNotification { .. }
        )));
        assert!(has_effect(&effects, |e| matches!(
            e,
            AppEffect::PersistSettings
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
            AppEffect::SyncAutoLaunch(true)
        )));

        // disable
        let effects = reduce(
            &mut session,
            AppAction::UpdateSetting(SettingChange::ToggleStartAtLogin),
        );
        assert!(!session.settings.system.start_at_login);
        assert!(has_effect(&effects, |e| matches!(
            e,
            AppEffect::SyncAutoLaunch(false)
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
            AppEffect::PersistSettings
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

    // ── DebugRefreshProvider ────────────────────────────

    #[test]
    fn debug_refresh_without_selection_is_noop() {
        let mut session = make_session();
        let effects = reduce(&mut session, AppAction::DebugRefreshProvider);

        assert!(!session.debug_ui.refresh_active);
        assert!(!has_effect(&effects, |e| matches!(
            e,
            AppEffect::StartDebugRefresh(_)
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
            AppEffect::StartDebugRefresh(_)
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
            AppEffect::StartDebugRefresh(_)
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
            AppEffect::ApplyTrayIcon(TrayIconRequest::Static(TrayIconStyle::Yellow))
        )));
        assert!(has_effect(&effects, |e| matches!(
            e,
            AppEffect::PersistSettings
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
            AppEffect::ApplyTrayIcon(TrayIconRequest::DynamicStatus(StatusLevel::Green))
        )));
    }

    #[test]
    fn refresh_success_in_dynamic_mode_produces_tray_icon_effect() {
        use crate::models::{QuotaInfo, StatusLevel, TrayIconStyle};

        let mut session = make_session();
        session.settings.display.tray_icon_style = TrayIconStyle::Dynamic;
        session
            .settings
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
                    },
                },
            })),
        );

        // 当前 Provider Claude 变 Red → 产出 ApplyTrayIcon
        assert!(has_effect(&effects, |e| matches!(
            e,
            AppEffect::ApplyTrayIcon(TrayIconRequest::DynamicStatus(StatusLevel::Red))
        )));
    }

    #[test]
    fn refresh_success_in_static_mode_does_not_produce_tray_icon_effect() {
        use crate::models::{QuotaInfo, TrayIconStyle};

        let mut session = make_session();
        session.settings.display.tray_icon_style = TrayIconStyle::Yellow; // 静态模式
        session
            .settings
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
                    },
                },
            })),
        );

        // 静态模式下不应产出 ApplyTrayIcon effect
        assert!(!has_effect(&effects, |e| matches!(
            e,
            AppEffect::ApplyTrayIcon(_)
        )));
    }

    #[test]
    fn refresh_success_in_dynamic_mode_no_effect_when_status_unchanged() {
        use crate::models::{QuotaInfo, TrayIconStyle};

        let mut session = make_session();
        session.settings.display.tray_icon_style = TrayIconStyle::Dynamic;
        session
            .settings
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
                    },
                },
            })),
        );

        assert!(!has_effect(&effects, |e| matches!(
            e,
            AppEffect::ApplyTrayIcon(_)
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
                    },
                },
            })),
        );

        // 非当前 Provider 的刷新不影响图标
        assert!(!has_effect(&effects, |e| matches!(
            e,
            AppEffect::ApplyTrayIcon(_)
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
                    },
                },
            })),
        );

        // 弹窗可见时不产出 ApplyTrayIcon
        assert!(!has_effect(&effects, |e| matches!(
            e,
            AppEffect::ApplyTrayIcon(_)
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
                    },
                },
            })),
        );

        // 关闭弹窗 → 同步图标
        let effects = reduce(&mut session, AppAction::PopupVisibilityChanged(false));

        assert!(!session.popup_visible);
        assert!(has_effect(&effects, |e| matches!(
            e,
            AppEffect::ApplyTrayIcon(TrayIconRequest::DynamicStatus(StatusLevel::Red))
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
            AppEffect::ApplyTrayIcon(_)
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
            AppEffect::ApplyTrayIcon(_)
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

        // Effect: SaveCustomProviderYaml（检查文件名和内容包含关键字段）
        assert!(has_effect(&effects, |e| {
            matches!(e, AppEffect::SaveCustomProviderYaml { filename, yaml_content }
                if filename.starts_with("newapi-")
                && filename.ends_with(".yaml")
                && yaml_content.contains("Test Site")
                && yaml_content.contains("https://api.example.com")
                && yaml_content.contains("session=tok_123")
                && yaml_content.contains("/api/user/self")
                && yaml_content.contains("New-Api-User")
                && yaml_content.contains("42")
                && yaml_content.contains("1000000")
            )
        }));

        // Effect: SendPlainNotification（通知用户重启）
        assert!(has_effect(&effects, |e| matches!(
            e,
            AppEffect::SendPlainNotification { .. }
        )));

        assert!(has_render(&effects));
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
            matches!(e, AppEffect::SaveCustomProviderYaml { yaml_content, .. }
                if yaml_content.contains("/api/user/self")
                && yaml_content.contains("divisor: 500000")
            )
        }));
    }

    #[test]
    fn select_provider_resets_adding_newapi() {
        let mut session = make_session();
        session.settings_ui.adding_newapi = true;

        let id = session.provider_store.providers[0].provider_id.clone();
        let effects = reduce(&mut session, AppAction::SelectSettingsProvider(id));

        assert!(!session.settings_ui.adding_newapi);
        assert!(has_render(&effects));
    }

    // ── 编辑模式 ──────────────────────────────────────

    #[test]
    fn submit_newapi_in_edit_mode_uses_original_filename() {
        use crate::providers::custom::generator::NewApiEditData;

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

        // Effect: 使用原始文件名而非重新生成
        assert!(has_effect(&effects, |e| {
            matches!(e, AppEffect::SaveCustomProviderYaml { filename, yaml_content }
                if filename == "original-file.yaml"
                && yaml_content.contains("Updated Name")
                && yaml_content.contains("new_cookie")
            )
        }));

        // 通知应包含编辑相关内容
        assert!(has_effect(&effects, |e| matches!(
            e,
            AppEffect::SendPlainNotification { .. }
        )));
    }

    #[test]
    fn cancel_add_newapi_clears_editing_state() {
        use crate::providers::custom::generator::NewApiEditData;

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
        use crate::providers::custom::generator::NewApiEditData;

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
            AppEffect::PersistSettings
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
            .is_quota_visible(ProviderKind::Claude, "session"));
        assert!(has_effect(&effects, |e| matches!(
            e,
            AppEffect::PersistSettings
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
            .is_quota_visible(ProviderKind::Claude, "weekly"));
    }

    // ── ClearDebugLogs ──────────────────────────────────

    #[test]
    fn clear_debug_logs_produces_effect() {
        let mut session = make_session();
        let effects = reduce(&mut session, AppAction::ClearDebugLogs);

        assert!(has_effect(&effects, |e| matches!(
            e,
            AppEffect::ClearDebugLogs
        )));
        assert!(has_render(&effects));
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
                error: "test error".to_string(),
                error_kind: crate::models::ErrorKind::NetworkError,
            },
        };
        let mut effects = vec![];
        apply_refresh_event(&mut session, RefreshEvent::Finished(outcome), &mut effects);

        assert!(!session.debug_ui.refresh_active);
        assert!(session.debug_ui.prev_log_level.is_none());
        assert!(has_effect(&effects, |e| matches!(
            e,
            AppEffect::RestoreLogLevel(log::LevelFilter::Info)
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
        let mut effects = vec![];
        apply_refresh_event(&mut session, RefreshEvent::Finished(outcome), &mut effects);

        assert!(session.debug_ui.refresh_active);
        assert!(session.debug_ui.prev_log_level.is_some());
        assert!(!has_effect(&effects, |e| matches!(
            e,
            AppEffect::RestoreLogLevel(_)
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
                error: "gone".to_string(),
                error_kind: crate::models::ErrorKind::Unknown,
            },
        };
        let mut effects = vec![];
        apply_refresh_event(&mut session, RefreshEvent::Finished(outcome), &mut effects);

        assert!(!session.debug_ui.refresh_active);
        assert!(has_effect(&effects, |e| matches!(
            e,
            AppEffect::RestoreLogLevel(log::LevelFilter::Warn)
        )));
    }

    // ── ProvidersReloaded (热重载) ───────────────────────────

    fn make_custom_provider_status(id: &str) -> crate::models::ProviderStatus {
        let provider_id = ProviderId::Custom(id.to_string());
        let metadata = crate::models::test_helpers::make_test_metadata(ProviderKind::Custom);
        crate::models::ProviderStatus::new_custom(provider_id, metadata)
    }

    #[test]
    fn providers_reloaded_sends_update_config() {
        let mut session = make_session();
        let statuses: Vec<_> = session.provider_store.providers.iter().cloned().collect();

        let mut effects = vec![];
        apply_refresh_event(
            &mut session,
            RefreshEvent::ProvidersReloaded { statuses },
            &mut effects,
        );

        assert!(has_effect(&effects, |e| matches!(
            e,
            AppEffect::SendRefreshRequest(RefreshRequest::UpdateConfig { .. })
        )));
        assert!(has_render(&effects));
    }

    #[test]
    fn providers_reloaded_refreshes_enabled_new_custom() {
        let mut session = make_session();
        let custom_id = ProviderId::Custom("new:api".to_string());
        session.settings.set_enabled(&custom_id, true);

        let mut statuses: Vec<_> = session.provider_store.providers.iter().cloned().collect();
        statuses.push(make_custom_provider_status("new:api"));

        let mut effects = vec![];
        apply_refresh_event(
            &mut session,
            RefreshEvent::ProvidersReloaded { statuses },
            &mut effects,
        );

        assert!(has_effect(&effects, |e| matches!(
            e,
            AppEffect::SendRefreshRequest(RefreshRequest::RefreshOne {
                ref id,
                ..
            }) if *id == ProviderId::Custom("new:api".to_string())
        )));
    }

    #[test]
    fn providers_reloaded_does_not_refresh_disabled_custom() {
        let mut session = make_session();

        let mut statuses: Vec<_> = session.provider_store.providers.iter().cloned().collect();
        statuses.push(make_custom_provider_status("disabled:api"));

        let mut effects = vec![];
        apply_refresh_event(
            &mut session,
            RefreshEvent::ProvidersReloaded { statuses },
            &mut effects,
        );

        assert!(!has_effect(&effects, |e| matches!(
            e,
            AppEffect::SendRefreshRequest(RefreshRequest::RefreshOne {
                ref id,
                ..
            }) if *id == ProviderId::Custom("disabled:api".to_string())
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

        let mut effects = vec![];
        apply_refresh_event(
            &mut session,
            RefreshEvent::ProvidersReloaded { statuses },
            &mut effects,
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
        session.settings.set_enabled(&custom_id, true);
        session.nav.switch_to(NavTab::Provider(custom_id.clone()));

        let statuses: Vec<_> = ProviderKind::all()
            .iter()
            .map(|k| make_test_provider(*k, ConnectionStatus::Disconnected))
            .collect();

        let mut effects = vec![];
        apply_refresh_event(
            &mut session,
            RefreshEvent::ProvidersReloaded { statuses },
            &mut effects,
        );

        match &session.nav.active_tab {
            NavTab::Provider(id) => assert_ne!(*id, custom_id),
            NavTab::Settings => {}
        }
    }

    #[test]
    fn providers_reloaded_persists_settings_when_stale_ids_pruned() {
        let mut session = make_session();
        let custom_id = ProviderId::Custom("stale:api".to_string());
        session.settings.set_enabled(&custom_id, true);
        session
            .provider_store
            .providers
            .push(make_custom_provider_status("stale:api"));

        let statuses: Vec<_> = ProviderKind::all()
            .iter()
            .map(|k| make_test_provider(*k, ConnectionStatus::Disconnected))
            .collect();

        let mut effects = vec![];
        apply_refresh_event(
            &mut session,
            RefreshEvent::ProvidersReloaded { statuses },
            &mut effects,
        );

        assert!(has_effect(&effects, |e| matches!(
            e,
            AppEffect::PersistSettings
        )));
    }
}
