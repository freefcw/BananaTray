use crate::application::{AppEffect, ContextEffect, TrayIconRequest};
use crate::models::{ProviderId, StatusLevel, TrayIconStyle};
use crate::refresh::RefreshRequest;

use super::super::state::AppSession;

pub fn build_config_sync_request(session: &AppSession) -> RefreshRequest {
    let enabled: Vec<ProviderId> = session
        .provider_store
        .providers
        .iter()
        .filter(|p| session.settings.provider.is_enabled(&p.provider_id) && p.supports_refresh())
        .map(|p| p.provider_id.clone())
        .collect();

    RefreshRequest::UpdateConfig {
        interval_mins: session.settings.system.refresh_interval_mins,
        enabled,
        provider_credentials: session.settings.provider.credentials.clone(),
    }
}

pub(super) fn provider_supports_refresh(session: &AppSession, id: &ProviderId) -> bool {
    session
        .provider_store
        .find_by_id(id)
        .is_some_and(|provider| provider.supports_refresh())
}

/// 将用户选择的 TrayIconStyle 解析为具体的 TrayIconRequest。
/// Dynamic 模式时根据当前 Provider 状态计算颜色，其余模式直接映射为静态请求。
pub(super) fn resolve_tray_icon_request(
    session: &AppSession,
    style: TrayIconStyle,
) -> TrayIconRequest {
    if style == TrayIconStyle::Dynamic {
        TrayIconRequest::DynamicStatus(session.current_provider_status())
    } else {
        TrayIconRequest::Static(style)
    }
}

/// 若处于 Dynamic 模式，且刷新的是当前 Provider，且弹窗不可见，且状态发生变化时，
/// 追加 ApplyTrayIcon effect。
pub(super) fn sync_dynamic_icon_if_needed(
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
