use super::state::{AppSession, SettingsTab};
use crate::application::{AppAction, AppEffect, SettingChange, TrayIconRequest};
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
            session.settings_ui.confirming_remove_provider = false;
            session.settings_ui.confirming_delete_newapi = false;
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
        AppAction::MoveProviderToIndex { id, target_index } => {
            let custom_ids = session.provider_store.custom_provider_ids();
            let moved =
                session
                    .settings
                    .provider
                    .move_provider_to_index(&id, target_index, &custom_ids);
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
        AppAction::EnterAddProvider => {
            session.settings_ui.adding_provider = true;
            session.settings_ui.adding_newapi = false; // 互斥
            push_render(&mut effects);
        }
        AppAction::CancelAddProvider => {
            session.settings_ui.adding_provider = false;
            push_render(&mut effects);
        }
        AppAction::AddProviderToSidebar(id) => {
            if session.settings.provider.add_to_sidebar(&id) {
                effects.push(AppEffect::PersistSettings);
            }
            session.settings_ui.adding_provider = false;
            session.settings_ui.selected_provider = id;
            push_render(&mut effects);
        }
        AppAction::RemoveProviderFromSidebar(id) => {
            session.settings_ui.confirming_remove_provider = false;
            if session.settings.provider.remove_from_sidebar(&id) {
                // 移除同时 disable
                session.settings.provider.set_enabled(&id, false);
                // 导航回退
                let providers = &session.provider_store.providers;
                session
                    .nav
                    .fallback_on_disable(&id, providers, &session.settings);
                // 选中下一个可用项
                let custom_ids = session.provider_store.custom_provider_ids();
                let remaining = session.settings.provider.sidebar_provider_ids(&custom_ids);
                if let Some(first) = remaining.first() {
                    session.settings_ui.selected_provider = first.clone();
                } else {
                    session.settings_ui.adding_provider = true;
                }
                effects.push(AppEffect::PersistSettings);
                effects.push(AppEffect::SendRefreshRequest(build_config_sync_request(
                    session,
                )));
            }
            push_render(&mut effects);
        }
        AppAction::ConfirmRemoveProvider => {
            session.settings_ui.confirming_remove_provider = true;
            push_render(&mut effects);
        }
        AppAction::CancelRemoveProvider => {
            session.settings_ui.confirming_remove_provider = false;
            push_render(&mut effects);
        }
        AppAction::EnterAddNewApi => {
            session.settings_ui.adding_newapi = true;
            session.settings_ui.adding_provider = false; // 与 picker 互斥
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
        AppAction::DeleteNewApi { provider_id } => {
            use crate::providers::custom::generator;

            session.settings_ui.confirming_delete_newapi = false;
            if let ProviderId::Custom(ref custom_id) = provider_id {
                if let Some(filename) = generator::filename_for_id(custom_id) {
                    effects.push(AppEffect::DeleteCustomProviderYaml { filename });
                } else {
                    log::warn!(
                        target: "settings",
                        "DeleteNewApi: not a newapi provider id: {}",
                        custom_id
                    );
                }
            }
        }
        AppAction::ConfirmDeleteNewApi => {
            session.settings_ui.confirming_delete_newapi = true;
            push_render(&mut effects);
        }
        AppAction::CancelDeleteNewApi => {
            session.settings_ui.confirming_delete_newapi = false;
            push_render(&mut effects);
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
            session
                .settings
                .provider
                .toggle_quota_visibility(kind, quota_key);
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
    if !session.settings.provider.is_enabled(&id) {
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
    let new_val = !session.settings.provider.is_enabled(&id);
    info!(
        target: "providers",
        "toggling provider {} from {} to {}",
        id,
        !new_val,
        new_val
    );
    session.settings.provider.set_enabled(&id, new_val);

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
                if session.settings.provider.is_enabled(id) {
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
        .filter(|p| session.settings.provider.is_enabled(&p.provider_id))
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
            .find(|p| session.settings.provider.is_enabled(&p.provider_id))
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
#[path = "reducer_tests.rs"]
mod tests;
