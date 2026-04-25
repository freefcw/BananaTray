use log::{debug, info};

use crate::application::{
    AppEffect, ContextEffect, DebugEffect, NotificationEffect, RefreshEffect, SettingsEffect,
};
use crate::models::{NavTab, ProviderId, ProviderKind};
use crate::refresh::{RefreshEvent, RefreshReason, RefreshRequest, RefreshResult};

use super::super::state::AppSession;
use super::shared::{
    build_config_sync_request, provider_supports_refresh, sync_dynamic_icon_if_needed,
};

pub(super) fn refresh_all_providers(session: &mut AppSession, effects: &mut Vec<AppEffect>) {
    let enabled_ids: Vec<ProviderId> = session
        .provider_store
        .providers
        .iter()
        .filter(|p| session.settings.provider.is_enabled(&p.provider_id) && p.supports_refresh())
        .map(|p| p.provider_id.clone())
        .collect();

    if enabled_ids.is_empty() {
        return;
    }

    for id in &enabled_ids {
        session.provider_store.mark_refreshing_by_id(id);
    }

    effects.push(
        RefreshEffect::SendRequest(RefreshRequest::RefreshAll {
            reason: RefreshReason::Manual,
        })
        .into(),
    );
    effects.push(ContextEffect::Render.into());
}

pub(super) fn request_provider_refresh(
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

    if !provider_supports_refresh(session, &id) {
        debug!(
            target: "refresh",
            "ignoring refresh request for non-monitorable provider {}",
            id
        );
        return;
    }

    session.provider_store.mark_refreshing_by_id(&id);
    effects.push(RefreshEffect::SendRequest(RefreshRequest::RefreshOne { id, reason }).into());
    effects.push(ContextEffect::Render.into());
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
                        NotificationEffect::Quota {
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
        RefreshResult::Unavailable { failure } => {
            debug!(
                target: "providers",
                "provider {} unavailable: {:?}",
                outcome_id,
                failure
            );
            if let Some(provider) = session.provider_store.find_by_id_mut(outcome_id) {
                provider.mark_unavailable(failure);
                effects.push(ContextEffect::Render.into());
            }
        }
        RefreshResult::Failed {
            failure,
            error_kind,
        } => {
            if let Some(provider) = session.provider_store.find_by_id_mut(outcome_id) {
                provider.mark_refresh_failed(failure, error_kind);
                effects.push(ContextEffect::Render.into());
            }
        }
        RefreshResult::SkippedCooldown
        | RefreshResult::SkippedInFlight
        | RefreshResult::SkippedDisabled => {}
    }
}

pub(super) fn apply_refresh_event(
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
                    effects.push(DebugEffect::RestoreLogLevel(prev_level).into());
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
                effects.push(SettingsEffect::PersistSettings.into());
            }

            // 自动启用首次出现的自定义 Provider（热重载发现的新 YAML 文件）
            let auto_registered = session
                .settings
                .provider
                .register_discovered_custom_providers(&affected);
            for id in &auto_registered {
                info!(
                    target: "providers",
                    "auto-enabled new custom provider: {}",
                    id
                );
            }
            if !auto_registered.is_empty() {
                effects.push(SettingsEffect::PersistSettings.into());
            }

            // 清理可能指向已删除 provider 的导航/设置引用
            cleanup_dangling_refs(session);

            // 同步 coordinator 的 enabled 列表
            effects.push(RefreshEffect::SendRequest(build_config_sync_request(session)).into());

            // 对新增/更新的自定义 Provider 立即触发刷新
            for id in &affected {
                if session.settings.provider.is_enabled(id)
                    && session
                        .provider_store
                        .find_by_id(id)
                        .is_some_and(|provider| provider.supports_refresh())
                {
                    session.provider_store.mark_refreshing_by_id(id);
                    effects.push(
                        RefreshEffect::SendRequest(RefreshRequest::RefreshOne {
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
            .unwrap_or(ProviderId::BuiltIn(ProviderKind::Claude));
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
