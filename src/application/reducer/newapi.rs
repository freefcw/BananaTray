use crate::application::{newapi_ops, AppEffect, ContextEffect, NewApiEffect, RefreshEffect};
use crate::models::{
    CustomProviderLifecycleFailure, NewApiConfig, NewApiEditData, NewApiSaveSuccess, ProviderId,
};
use crate::refresh::RefreshRequest;

use super::super::state::{AppSession, SettingsModalState};

pub(super) fn enter_add_newapi(session: &mut AppSession, effects: &mut Vec<AppEffect>) {
    // 进入新增表单时直接覆盖其他模态（picker / 旧的编辑回填 / 脚本表单）
    session.settings_ui.modal = SettingsModalState::AddingNewApi;
    session.settings_ui.token_editing_provider = None;
    effects.push(ContextEffect::Render.into());
}

pub(super) fn cancel_add_newapi(session: &mut AppSession, effects: &mut Vec<AppEffect>) {
    if session.settings_ui.modal.is_newapi_form() {
        session.settings_ui.modal = SettingsModalState::Idle;
    }
    session.settings_ui.token_editing_provider = None;
    effects.push(ContextEffect::Render.into());
}

pub(super) fn submit_newapi(
    session: &mut AppSession,
    mut config: NewApiConfig,
    effects: &mut Vec<AppEffect>,
) {
    let (base_url, original_filename, original_id, is_editing) =
        match session.settings_ui.modal.newapi_edit_data() {
            Some(edit_data) => (
                edit_data.base_url.clone(),
                Some(edit_data.original_filename.clone()),
                Some(edit_data.original_id.clone()),
                true,
            ),
            None => (config.base_url.clone(), None, None, false),
        };
    config.base_url = base_url.clone();

    // ── Provider 身份：编辑模式保持原始 ID 不变；新增按 base_url + user_id 计算，
    //    同一站点可通过不同 user_id 添加多个账号 ──
    // 编辑模式使用 EditingNewApi 中的原始 URL / ID，Provider 身份不受 action payload 影响。
    let new_id = ProviderId::Custom(match &original_id {
        Some(id) => id.clone(),
        None => crate::models::newapi_provider_id(&base_url, config.user_id.as_deref()),
    });

    // ── 新增冲突守卫：同站点同账号已存在时拒绝静默覆盖，引导用户改用编辑 ──
    if !is_editing && newapi_identity_occupied(session, &new_id) {
        let (title_key, body_key) = newapi_ops::newapi_duplicate_notification_keys();
        super::shared::notify_plain_i18n(effects, title_key, body_key);
        // 表单保持打开，用户输入不丢失
        effects.push(ContextEffect::Render.into());
        return;
    }

    // ── 预注册 Provider ID：确保热重载后 Provider 立即可见 ──
    if !session
        .settings
        .provider
        .enabled_providers
        .contains_key(&new_id.id_key())
    {
        session.settings.provider.set_enabled(&new_id, true);
    }
    session.settings.provider.add_to_sidebar(&new_id);
    session.settings_ui.selected_provider = new_id;

    effects.push(
        NewApiEffect::SaveProvider {
            config,
            original_filename,
            original_id,
            is_editing,
        }
        .into(),
    );
    // 保存 I/O 完成后由 NewApiSaveFinished 统一处理成功通知、reload 或失败回滚，
    // 避免写入失败时产生幽灵 Provider 或虚假成功通知。
    session.settings_ui.modal = SettingsModalState::Idle;
    session.settings_ui.token_editing_provider = None;
    effects.push(ContextEffect::Render.into());
}

/// 新增模式下判断身份（站点 + 账号）是否已被占用：
/// settings 中的启用记录（含被禁用的记录）或已加载的 Provider 均视为占用。
fn newapi_identity_occupied(session: &AppSession, id: &ProviderId) -> bool {
    session
        .settings
        .provider
        .enabled_providers
        .contains_key(&id.id_key())
        || session.provider_store.find_by_id(id).is_some()
}

pub(super) fn newapi_save_finished(
    session: &mut AppSession,
    config: NewApiConfig,
    filename: String,
    original_id: Option<String>,
    is_editing: bool,
    result: Result<NewApiSaveSuccess, CustomProviderLifecycleFailure>,
    effects: &mut Vec<AppEffect>,
) {
    match result {
        Ok(success) => {
            let (title_key, body_key) =
                newapi_ops::newapi_save_notification_keys(is_editing, success.settings_saved);
            super::shared::notify_plain_i18n(effects, title_key, body_key);
            effects.push(RefreshEffect::SendRequest(RefreshRequest::ReloadProviders).into());
        }
        Err(_failure) => {
            if is_editing {
                newapi_ops::rollback_newapi_edit(
                    session,
                    &config,
                    &filename,
                    original_id.as_deref(),
                );
            } else {
                newapi_ops::rollback_newapi_create(session, &config);
            }
            let (title_key, body_key) = newapi_ops::newapi_save_failed_notification_keys();
            super::shared::notify_plain_i18n(effects, title_key, body_key);
            effects.push(ContextEffect::Render.into());
        }
    }
}

pub(super) fn newapi_load_finished(
    session: &mut AppSession,
    _provider_id: ProviderId,
    result: Result<NewApiEditData, CustomProviderLifecycleFailure>,
    effects: &mut Vec<AppEffect>,
) {
    match result {
        Ok(edit_data) => {
            session.settings_ui.modal = SettingsModalState::EditingNewApi(edit_data);
        }
        Err(_failure) => {
            let (title_key, body_key) = newapi_ops::newapi_load_failed_notification_keys();
            super::shared::notify_plain_i18n(effects, title_key, body_key);
        }
    }
    effects.push(ContextEffect::Render.into());
}

pub(super) fn newapi_delete_finished(
    _session: &mut AppSession,
    _provider_id: ProviderId,
    result: Result<std::path::PathBuf, CustomProviderLifecycleFailure>,
    effects: &mut Vec<AppEffect>,
) {
    match result {
        Ok(_path) => {
            effects.push(RefreshEffect::SendRequest(RefreshRequest::ReloadProviders).into())
        }
        Err(_failure) => {
            super::shared::notify_plain_i18n(
                effects,
                "newapi.delete_failed_title",
                "newapi.delete_failed_body",
            );
        }
    }
}

pub(super) fn edit_newapi(
    session: &mut AppSession,
    provider_id: ProviderId,
    effects: &mut Vec<AppEffect>,
) {
    // 磁盘 I/O 委托给 runtime effect handler，保持 reducer 纯函数
    // 切到 NewAPI 编辑面板时，token 编辑上下文需要结束。
    session.settings_ui.token_editing_provider = None;
    effects.push(NewApiEffect::LoadConfig { provider_id }.into());
    effects.push(ContextEffect::Render.into());
}

pub(super) fn delete_newapi(
    session: &mut AppSession,
    provider_id: ProviderId,
    effects: &mut Vec<AppEffect>,
) {
    if session.settings_ui.modal.is_confirming_delete_newapi() {
        session.settings_ui.modal = SettingsModalState::Idle;
    }
    session.settings_ui.token_editing_provider = None;
    // 先刷新 UI 关闭确认态，避免等待文件删除 / 热重载结果才消失。
    effects.push(ContextEffect::Render.into());
    effects.push(NewApiEffect::DeleteProvider { provider_id }.into());
}

pub(super) fn confirm_delete_newapi(session: &mut AppSession, effects: &mut Vec<AppEffect>) {
    session.settings_ui.modal = SettingsModalState::ConfirmingDeleteNewApi;
    session.settings_ui.token_editing_provider = None;
    effects.push(ContextEffect::Render.into());
}

pub(super) fn cancel_delete_newapi(session: &mut AppSession, effects: &mut Vec<AppEffect>) {
    if session.settings_ui.modal.is_confirming_delete_newapi() {
        session.settings_ui.modal = SettingsModalState::Idle;
    }
    session.settings_ui.token_editing_provider = None;
    effects.push(ContextEffect::Render.into());
}
