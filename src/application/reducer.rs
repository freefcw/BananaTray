use super::state::{AppSession, SettingsTab};
use crate::application::{
    AppAction, AppEffect, CommonEffect, ContextEffect, SettingChange, TrayIconRequest,
};
use crate::models::{NavTab, ProviderId, StatusLevel, TrayIconStyle};
use crate::refresh::{RefreshEvent, RefreshReason, RefreshRequest, RefreshResult};
use log::{debug, info};

pub fn reduce(session: &mut AppSession, action: AppAction) -> Vec<AppEffect> {
    let mut effects = Vec::new();

    match action {
        AppAction::SelectNavTab(tab) => {
            session.nav.switch_to(tab);
            effects.push(ContextEffect::Render.into());
        }
        AppAction::SetSettingsTab(tab) => {
            session.settings_ui.active_tab = tab;
            effects.push(ContextEffect::Render.into());
        }
        AppAction::SelectSettingsProvider(id) => {
            session.settings_ui.selected_provider = id;
            session.settings_ui.adding_newapi = false;
            session.settings_ui.confirming_remove_provider = false;
            session.settings_ui.confirming_delete_newapi = false;
            effects.push(ContextEffect::Render.into());
        }
        AppAction::ToggleCadenceDropdown => {
            session.settings_ui.cadence_dropdown_open = !session.settings_ui.cadence_dropdown_open;
            effects.push(ContextEffect::Render.into());
        }
        AppAction::SetCopilotTokenEditing(editing) => {
            session.settings_ui.copilot_token_editing = editing;
            effects.push(ContextEffect::Render.into());
        }
        AppAction::SaveCopilotToken(token) => {
            let token = token.trim().to_string();
            if !token.is_empty() {
                session.settings.provider.credentials.github_token = Some(token);
                effects.push(CommonEffect::PersistSettings.into());
            }
            session.settings_ui.copilot_token_editing = false;
            effects.push(ContextEffect::Render.into());
        }
        AppAction::MoveProviderToIndex { id, target_index } => {
            let custom_ids = session.provider_store.custom_provider_ids();
            let moved =
                session
                    .settings
                    .provider
                    .move_provider_to_index(&id, target_index, &custom_ids);
            if moved {
                effects.push(CommonEffect::PersistSettings.into());
                effects.push(ContextEffect::Render.into());
            }
        }
        AppAction::UpdateSetting(change) => {
            apply_setting_change(session, change, &mut effects);
        }
        AppAction::RefreshProvider { id, reason } => {
            request_provider_refresh(session, id, reason, &mut effects);
        }
        AppAction::RefreshAll => {
            refresh_all_providers(session, &mut effects);
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
            effects.push(ContextEffect::OpenSettingsWindow.into());
        }
        AppAction::OpenDashboard(id) => {
            if let Some(provider) = session.provider_store.find_by_id(&id) {
                let url = provider.dashboard_url().trim();
                if !url.is_empty() {
                    effects.push(ContextEffect::OpenUrl(url.to_string()).into());
                }
            }
        }
        AppAction::OpenUrl(url) => effects.push(ContextEffect::OpenUrl(url).into()),
        AppAction::UpdateLogLevel(level) => {
            effects.push(CommonEffect::UpdateLogLevel(level).into());
            effects.push(ContextEffect::Render.into());
        }
        AppAction::SendDebugNotification(kind) => {
            effects.push(
                CommonEffect::SendDebugNotification {
                    kind,
                    with_sound: session.settings.notification.notification_sound,
                }
                .into(),
            );
        }
        AppAction::OpenLogDirectory => {
            effects.push(CommonEffect::OpenLogDirectory.into());
        }
        AppAction::CopyToClipboard(text) => {
            effects.push(CommonEffect::CopyToClipboard(text).into());
        }
        AppAction::SelectDebugProvider(id) => {
            session.debug_ui.selected_provider = Some(id);
            effects.push(ContextEffect::Render.into());
        }
        AppAction::DebugRefreshProvider => {
            if let Some(ref id) = session.debug_ui.selected_provider {
                if !session.debug_ui.refresh_active {
                    session.debug_ui.refresh_active = true;
                    session.provider_store.mark_refreshing_by_id(id);
                    effects.push(CommonEffect::StartDebugRefresh(id.clone()).into());
                    effects.push(ContextEffect::Render.into());
                }
            }
        }
        AppAction::ClearDebugLogs => {
            effects.push(CommonEffect::ClearDebugLogs.into());
            effects.push(ContextEffect::Render.into());
        }
        AppAction::PopupVisibilityChanged(visible) => {
            session.popup_visible = visible;
            if !visible {
                // 弹窗关闭时同步图标为当前 Provider 的状态
                if session.settings.display.tray_icon_style == TrayIconStyle::Dynamic {
                    effects.push(
                        ContextEffect::ApplyTrayIcon(TrayIconRequest::DynamicStatus(
                            session.current_provider_status(),
                        ))
                        .into(),
                    );
                }
            }
        }
        AppAction::EnterAddProvider => {
            session.settings_ui.adding_provider = true;
            session.settings_ui.adding_newapi = false; // 互斥
            effects.push(ContextEffect::Render.into());
        }
        AppAction::CancelAddProvider => {
            session.settings_ui.adding_provider = false;
            effects.push(ContextEffect::Render.into());
        }
        AppAction::AddProviderToSidebar(id) => {
            if session.settings.provider.add_to_sidebar(&id) {
                effects.push(CommonEffect::PersistSettings.into());
            }
            session.settings_ui.adding_provider = false;
            session.settings_ui.selected_provider = id;
            effects.push(ContextEffect::Render.into());
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
                effects.push(CommonEffect::PersistSettings.into());
                effects.push(
                    CommonEffect::SendRefreshRequest(build_config_sync_request(session)).into(),
                );
            }
            effects.push(ContextEffect::Render.into());
        }
        AppAction::ConfirmRemoveProvider => {
            session.settings_ui.confirming_remove_provider = true;
            effects.push(ContextEffect::Render.into());
        }
        AppAction::CancelRemoveProvider => {
            session.settings_ui.confirming_remove_provider = false;
            effects.push(ContextEffect::Render.into());
        }
        AppAction::EnterAddNewApi => {
            session.settings_ui.adding_newapi = true;
            session.settings_ui.adding_provider = false; // 与 picker 互斥
            session.settings_ui.editing_newapi = None; // 确保进入纯新增模式
            effects.push(ContextEffect::Render.into());
        }
        AppAction::CancelAddNewApi => {
            session.settings_ui.adding_newapi = false;
            session.settings_ui.editing_newapi = None;
            effects.push(ContextEffect::Render.into());
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

            effects.push(
                CommonEffect::SaveCustomProviderYaml {
                    yaml_content,
                    filename,
                }
                .into(),
            );

            let (title_key, body_key) = if is_editing {
                ("newapi.edit_success_title", "newapi.edit_success_body")
            } else {
                ("newapi.save_success_title", "newapi.save_success_body")
            };
            effects.push(
                CommonEffect::SendPlainNotification {
                    title: rust_i18n::t!(title_key).to_string(),
                    body: rust_i18n::t!(body_key).to_string(),
                }
                .into(),
            );
            session.settings_ui.adding_newapi = false;
            session.settings_ui.editing_newapi = None;
            effects.push(ContextEffect::Render.into());
        }
        AppAction::EditNewApi { provider_id } => {
            use crate::providers::custom::generator;

            if let ProviderId::Custom(ref custom_id) = provider_id {
                if let Some(edit_data) = generator::read_newapi_config(custom_id) {
                    session.settings_ui.adding_newapi = true;
                    session.settings_ui.editing_newapi = Some(edit_data);
                    effects.push(ContextEffect::Render.into());
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
                    effects.push(CommonEffect::DeleteCustomProviderYaml { filename }.into());
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
            effects.push(ContextEffect::Render.into());
        }
        AppAction::CancelDeleteNewApi => {
            session.settings_ui.confirming_delete_newapi = false;
            effects.push(ContextEffect::Render.into());
        }
        AppAction::QuitApp => effects.push(ContextEffect::QuitApp.into()),
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
            effects.push(CommonEffect::SyncAutoLaunch(new_val).into());
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
            effects.push(CommonEffect::SendPlainNotification { title, body }.into());
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
            effects.push(CommonEffect::ApplyLocale(language).into());
        }
        SettingChange::RefreshCadence(mins) => {
            session.settings.system.refresh_interval_mins = mins.unwrap_or(0);
            session.settings_ui.cadence_dropdown_open = false;
            effects
                .push(CommonEffect::SendRefreshRequest(build_config_sync_request(session)).into());
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
        SettingChange::ToggleQuotaVisibility { kind, quota_key } => {
            session
                .settings
                .provider
                .toggle_quota_visibility(kind, quota_key);
        }
    }

    effects.push(CommonEffect::PersistSettings.into());
    effects.push(ContextEffect::Render.into());
}

fn refresh_all_providers(session: &mut AppSession, effects: &mut Vec<AppEffect>) {
    let enabled_ids: Vec<ProviderId> = session
        .provider_store
        .providers
        .iter()
        .filter(|p| session.settings.provider.is_enabled(&p.provider_id))
        .map(|p| p.provider_id.clone())
        .collect();

    for id in &enabled_ids {
        session.provider_store.mark_refreshing_by_id(id);
    }

    effects.push(
        CommonEffect::SendRefreshRequest(RefreshRequest::RefreshAll {
            reason: RefreshReason::Manual,
        })
        .into(),
    );
    effects.push(ContextEffect::Render.into());
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
    effects
        .push(CommonEffect::SendRefreshRequest(RefreshRequest::RefreshOne { id, reason }).into());
    effects.push(ContextEffect::Render.into());
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

    effects.push(CommonEffect::PersistSettings.into());
    effects.push(CommonEffect::SendRefreshRequest(build_config_sync_request(session)).into());
    if new_val {
        request_provider_refresh(session, id, RefreshReason::ProviderToggled, effects);
    } else {
        // Provider 被禁用后需重新计算动态图标
        if session.settings.display.tray_icon_style == TrayIconStyle::Dynamic
            && !session.popup_visible
        {
            let status = session.current_provider_status();
            effects
                .push(ContextEffect::ApplyTrayIcon(TrayIconRequest::DynamicStatus(status)).into());
        }
        effects.push(ContextEffect::Render.into());
    }
}

fn process_refresh_outcome(
    session: &mut AppSession,
    outcome_id: &ProviderId,
    result: RefreshResult,
    effects: &mut Vec<AppEffect>,
) {
    if session.provider_store.find_by_id(outcome_id).is_none() {
        return;
    }

    match result {
        RefreshResult::Success { data } => {
            info!(
                target: "providers",
                "provider {} refresh succeeded: {} quotas",
                outcome_id,
                data.quotas.len()
            );
            let provider_name = session
                .provider_store
                .find_by_id(outcome_id)
                .map(|provider| provider.display_name().to_string())
                .unwrap_or_else(|| format!("{}", outcome_id));
            if let Some(alert) =
                session
                    .alert_tracker
                    .update(outcome_id, &provider_name, &data.quotas)
            {
                if session.settings.notification.session_quota_notifications {
                    effects.push(
                        CommonEffect::SendQuotaNotification {
                            alert,
                            with_sound: session.settings.notification.notification_sound,
                        }
                        .into(),
                    );
                }
            }
            if let Some(provider) = session.provider_store.find_by_id_mut(outcome_id) {
                provider.mark_refresh_succeeded(data);
                effects.push(ContextEffect::Render.into());
            }
        }
        RefreshResult::Unavailable { message } => {
            debug!(
                target: "providers",
                "provider {} unavailable: {}",
                outcome_id,
                message
            );
            if let Some(provider) = session.provider_store.find_by_id_mut(outcome_id) {
                provider.mark_unavailable(message);
                effects.push(ContextEffect::Render.into());
            }
        }
        RefreshResult::Failed { error, error_kind } => {
            if let Some(provider) = session.provider_store.find_by_id_mut(outcome_id) {
                provider.mark_refresh_failed(error, error_kind);
                effects.push(ContextEffect::Render.into());
            }
        }
        RefreshResult::SkippedCooldown
        | RefreshResult::SkippedInFlight
        | RefreshResult::SkippedDisabled => {}
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
            effects.push(ContextEffect::Render.into());
        }
        RefreshEvent::Finished(outcome) => {
            let is_debug_target = session.debug_ui.refresh_active
                && session.debug_ui.selected_provider.as_ref() == Some(&outcome.id);

            // 快照刷新前的状态等级，用于判断刷新后是否需要更新图标
            let prev_status = session.current_provider_status();
            let outcome_id = outcome.id.clone();

            process_refresh_outcome(session, &outcome_id, outcome.result, effects);

            // 动态图标：仅当刷新的是当前 Provider 时才检查状态变化
            sync_dynamic_icon_if_needed(session, &outcome_id, prev_status, effects);

            if is_debug_target {
                session.debug_ui.refresh_active = false;
                if let Some(prev_level) = session.debug_ui.prev_log_level.take() {
                    effects.push(CommonEffect::RestoreLogLevel(prev_level).into());
                }
                effects.push(ContextEffect::Render.into());
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
                effects.push(CommonEffect::PersistSettings.into());
            }

            // 清理可能指向已删除 provider 的导航/设置引用
            cleanup_dangling_refs(session);

            // 同步 coordinator 的 enabled 列表
            effects
                .push(CommonEffect::SendRefreshRequest(build_config_sync_request(session)).into());

            // 对新增/更新的自定义 Provider 立即触发刷新
            for id in &affected {
                if session.settings.provider.is_enabled(id) {
                    session.provider_store.mark_refreshing_by_id(id);
                    effects.push(
                        CommonEffect::SendRefreshRequest(RefreshRequest::RefreshOne {
                            id: id.clone(),
                            reason: RefreshReason::ProviderToggled,
                        })
                        .into(),
                    );
                }
            }

            effects.push(ContextEffect::Render.into());
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
fn cleanup_dangling_refs(session: &mut AppSession) {
    // 导航：如果当前 active_tab 指向的 provider 已不存在，回退
    if let NavTab::Provider(ref id) = session.nav.active_tab {
        if !provider_exists(session, id) {
            if let Some(tab) = session.default_provider_tab() {
                session.nav.switch_to(tab);
            } else {
                session.nav.switch_to(NavTab::Settings);
            }
        }
    }
    // last_provider_id
    if !provider_exists(session, &session.nav.last_provider_id) {
        if let Some(first) = session
            .provider_store
            .providers
            .iter()
            .find(|p| session.settings.provider.is_enabled(&p.provider_id))
            .map(|p| p.provider_id.clone())
        {
            session.nav.last_provider_id = first;
        }
    }
    // 设置面板选中的 provider：回退到 sidebar 列表第一个，而非硬编码 Claude
    if !provider_exists(session, &session.settings_ui.selected_provider) {
        let custom_ids = session.provider_store.custom_provider_ids();
        let sidebar_ids = session.settings.provider.sidebar_provider_ids(&custom_ids);
        session.settings_ui.selected_provider = sidebar_ids
            .first()
            .cloned()
            .unwrap_or(ProviderId::BuiltIn(crate::models::ProviderKind::Claude));
    }
    // Debug 面板
    let reset_debug_provider = session
        .debug_ui
        .selected_provider
        .as_ref()
        .is_some_and(|id| !provider_exists(session, id));
    if reset_debug_provider {
        session.debug_ui.selected_provider = None;
    }
}

fn provider_exists(session: &AppSession, id: &ProviderId) -> bool {
    session.provider_store.find_by_id(id).is_some()
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
fn sync_dynamic_icon_if_needed(
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
        effects
            .push(ContextEffect::ApplyTrayIcon(TrayIconRequest::DynamicStatus(new_status)).into());
    }
}

#[cfg(test)]
#[path = "reducer_tests.rs"]
mod tests;
