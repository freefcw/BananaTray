use log::info;

use crate::application::{
    AppEffect, ContextEffect, RefreshEffect, SettingsEffect, TrayIconRequest,
};
use crate::models::{NavTab, ProviderId, SettingsCapability, TrayIconStyle};
use crate::refresh::RefreshReason;

use super::super::state::AppSession;
use super::refresh::request_provider_refresh;
use super::shared::{build_config_sync_request, provider_supports_refresh};

pub(super) fn select_settings_provider(
    session: &mut AppSession,
    id: ProviderId,
    effects: &mut Vec<AppEffect>,
) {
    // 中转站表单打开时忽略侧栏点击：
    // 避免 selected_provider 与表单编辑目标不一致的分叉状态
    if session.settings_ui.adding_newapi {
        return;
    }
    session.settings_ui.selected_provider = id;
    session.settings_ui.token_editing_provider = None;
    // 点选已有服务商时退出 picker（轻量操作）
    session.settings_ui.adding_provider = false;
    session.settings_ui.confirming_remove_provider = false;
    session.settings_ui.confirming_delete_newapi = false;
    effects.push(ContextEffect::Render.into());
}

pub(super) fn set_token_editing(
    session: &mut AppSession,
    provider_id: ProviderId,
    editing: bool,
    effects: &mut Vec<AppEffect>,
) {
    session.settings_ui.token_editing_provider = if editing { Some(provider_id) } else { None };
    effects.push(ContextEffect::Render.into());
}

pub(super) fn save_provider_token(
    session: &mut AppSession,
    provider_id: ProviderId,
    token: String,
    effects: &mut Vec<AppEffect>,
) {
    let token = token.trim().to_string();
    if !token.is_empty() {
        // 从 ProviderStatus 获取 credential_key，通用化 token 存储
        let credential_key = session
            .provider_store
            .find_by_id(&provider_id)
            .and_then(|p| match &p.settings_capability {
                SettingsCapability::TokenInput(config) => Some(config.credential_key),
                _ => None,
            });
        if let Some(key) = credential_key {
            session
                .settings
                .provider
                .credentials
                .set_credential(key, token);
            effects.push(SettingsEffect::PersistSettings.into());
            effects.push(RefreshEffect::SendRequest(build_config_sync_request(session)).into());
        }
    }
    session.settings_ui.token_editing_provider = None;
    effects.push(ContextEffect::Render.into());
}

pub(super) fn move_provider_to_index(
    session: &mut AppSession,
    id: ProviderId,
    target_index: usize,
    effects: &mut Vec<AppEffect>,
) {
    let custom_ids = session.provider_store.custom_provider_ids();
    let moved = session
        .settings
        .provider
        .move_provider_to_index(&id, target_index, &custom_ids);
    if moved {
        effects.push(SettingsEffect::PersistSettings.into());
        effects.push(ContextEffect::Render.into());
    }
}

pub(super) fn toggle_provider(
    session: &mut AppSession,
    id: ProviderId,
    effects: &mut Vec<AppEffect>,
) {
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

    effects.push(SettingsEffect::PersistSettings.into());
    effects.push(RefreshEffect::SendRequest(build_config_sync_request(session)).into());
    if new_val {
        if provider_supports_refresh(session, &id) {
            request_provider_refresh(session, id, RefreshReason::ProviderToggled, effects);
        } else {
            effects.push(ContextEffect::Render.into());
        }
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

pub(super) fn open_dashboard(session: &AppSession, id: ProviderId, effects: &mut Vec<AppEffect>) {
    if let Some(provider) = session.provider_store.find_by_id(&id) {
        let url = provider.dashboard_url().trim();
        if !url.is_empty() {
            effects.push(ContextEffect::OpenUrl(url.to_string()).into());
        }
    }
}

pub(super) fn enter_add_provider(session: &mut AppSession, effects: &mut Vec<AppEffect>) {
    session.settings_ui.adding_provider = true;
    session.settings_ui.adding_newapi = false; // 互斥
    effects.push(ContextEffect::Render.into());
}

pub(super) fn cancel_add_provider(session: &mut AppSession, effects: &mut Vec<AppEffect>) {
    session.settings_ui.adding_provider = false;
    effects.push(ContextEffect::Render.into());
}

pub(super) fn add_provider_to_sidebar(
    session: &mut AppSession,
    id: ProviderId,
    effects: &mut Vec<AppEffect>,
) {
    if session.settings.provider.add_to_sidebar(&id) {
        effects.push(SettingsEffect::PersistSettings.into());
    }
    session.settings_ui.adding_provider = false;
    session.settings_ui.selected_provider = id;
    effects.push(ContextEffect::Render.into());
}

pub(super) fn remove_provider_from_sidebar(
    session: &mut AppSession,
    id: ProviderId,
    effects: &mut Vec<AppEffect>,
) {
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
        if remaining.is_empty() {
            session.settings_ui.adding_provider = true;
        } else {
            session.settings_ui.selected_provider = session.first_sidebar_provider();
        }
        effects.push(SettingsEffect::PersistSettings.into());
        effects.push(RefreshEffect::SendRequest(build_config_sync_request(session)).into());
    }
    effects.push(ContextEffect::Render.into());
}

pub(super) fn confirm_remove_provider(session: &mut AppSession, effects: &mut Vec<AppEffect>) {
    session.settings_ui.confirming_remove_provider = true;
    effects.push(ContextEffect::Render.into());
}

pub(super) fn cancel_remove_provider(session: &mut AppSession, effects: &mut Vec<AppEffect>) {
    session.settings_ui.confirming_remove_provider = false;
    effects.push(ContextEffect::Render.into());
}
