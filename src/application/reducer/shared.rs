use crate::application::{
    AppEffect, ContextEffect, NotificationEffect, RefreshEffect, SettingsEffect, TrayIconRequest,
};
use crate::models::{ProviderId, StatusLevel, TrayIconStyle};
use crate::refresh::RefreshRequest;

use super::super::state::AppSession;

pub fn build_config_sync_request(session: &AppSession) -> RefreshRequest {
    let enabled = session
        .provider_store
        .refreshable_provider_ids(&session.settings);

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
/// Dynamic 模式时根据所有已启用 Provider 的综合状态计算颜色，其余模式直接映射为静态请求。
pub(super) fn resolve_tray_icon_request(
    session: &AppSession,
    style: TrayIconStyle,
) -> TrayIconRequest {
    if style == TrayIconStyle::Dynamic {
        TrayIconRequest::DynamicStatus(session.worst_enabled_provider_status())
    } else {
        TrayIconRequest::Static(style)
    }
}

/// 若处于 Dynamic 模式、弹窗不可见、且已启用 Provider 的综合状态发生变化时，
/// 追加 ApplyTrayIcon effect。任一 Provider 刷新完成都可能改变综合状态。
pub(super) fn sync_dynamic_icon_if_needed(
    session: &AppSession,
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
    let new_status = session.worst_enabled_provider_status();
    if new_status != prev_status {
        effects
            .push(ContextEffect::ApplyTrayIcon(TrayIconRequest::DynamicStatus(new_status)).into());
    }
}

/// reducer 集中选择用户可见结果时，追加一条普通 i18n 文本通知。
pub(super) fn notify_plain_i18n(
    effects: &mut Vec<AppEffect>,
    title_key: &'static str,
    body_key: &'static str,
) {
    effects.push(
        NotificationEffect::PlainI18n {
            title_key,
            body_key,
        }
        .into(),
    );
}

/// 自定义 Provider 保存差异点的聚合：把 NewAPI / Script 各自特有的回滚与通知选择
/// 收集成具名字段，而不是做为 `settle_custom_provider_save` 的一堆位置参数。
pub(super) struct ProviderSavePolicy<'a> {
    /// 编辑已有 provider 时的表单回滚
    pub rollback_edit: Box<dyn FnOnce(&mut AppSession) + 'a>,
    /// 新增 provider 时的预注册回滚
    pub rollback_create: Box<dyn FnOnce(&mut AppSession) + 'a>,
    /// 新增模式恢复空表单
    pub restore_create_form: Box<dyn FnOnce(&mut AppSession) + 'a>,
    /// 成功通知 key 选择器
    pub success_keys: fn(bool, bool) -> (&'static str, &'static str),
    /// 失败通知 key 选择器
    pub failure_keys: fn() -> (&'static str, &'static str),
}

/// 自定义 Provider 保存完成后的统一结算流程。
///
/// 新增/编辑两种 provider（NewAPI、Script）共用同一套“成功 → 通知 + Reload；
/// 失败 → 回滚 + 失败通知 + Render”的语义；差异点全部收在 `policy` 里，
/// 避免在每个 provider 的 reducer 里各写一遍相同的 match 骨架。
pub(super) fn settle_custom_provider_save<S: crate::models::CustomProviderSaveSuccess>(
    session: &mut AppSession,
    effects: &mut Vec<AppEffect>,
    request_id: u64,
    is_editing: bool,
    result: Result<S, crate::models::CustomProviderLifecycleFailure>,
    policy: ProviderSavePolicy<'_>,
) {
    let restore_submission_ui = session.settings_ui.settle_custom_provider_save(request_id);
    match result {
        Ok(success) => {
            let (title_key, body_key) = (policy.success_keys)(is_editing, success.settings_saved());
            notify_plain_i18n(effects, title_key, body_key);
            effects.push(RefreshEffect::SendRequest(RefreshRequest::ReloadProviders).into());
        }
        Err(_failure) => {
            if is_editing && restore_submission_ui {
                (policy.rollback_edit)(session);
            } else if !is_editing {
                (policy.rollback_create)(session);
                if restore_submission_ui {
                    (policy.restore_create_form)(session);
                }
                effects.push(SettingsEffect::PersistSettings.into());
            }
            let (title_key, body_key) = (policy.failure_keys)();
            notify_plain_i18n(effects, title_key, body_key);
            effects.push(ContextEffect::Render.into());
        }
    }
}

/// 文件删除成功后立即提交 settings 侧的删除，避免等待 reload 时发生 file/settings 分裂。
pub(super) fn commit_deleted_provider(
    session: &mut AppSession,
    provider_id: &ProviderId,
    effects: &mut Vec<AppEffect>,
) {
    session
        .settings
        .provider
        .remove_provider_references(provider_id);
    session.alert_engine.remove(provider_id);
    if session.settings_ui.selected_provider == *provider_id {
        session.settings_ui.selected_provider = session.first_sidebar_provider();
    }
    effects.push(SettingsEffect::PersistSettings.into());
    super::history::note_deleted_provider(session, provider_id, effects);
}
