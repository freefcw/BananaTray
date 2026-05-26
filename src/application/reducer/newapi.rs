use crate::application::{AppEffect, ContextEffect, NewApiEffect};
use crate::models::{NewApiConfig, ProviderId};

use super::super::state::{AppSession, SettingsModalState};

pub(super) fn enter_add_newapi(session: &mut AppSession, effects: &mut Vec<AppEffect>) {
    // 进入新增表单时直接覆盖其他模态（picker / 旧的编辑回填）
    session.settings_ui.modal = SettingsModalState::AddingNewApi;
    effects.push(ContextEffect::Render.into());
}

pub(super) fn cancel_add_newapi(session: &mut AppSession, effects: &mut Vec<AppEffect>) {
    if session.settings_ui.modal.is_newapi_form() {
        session.settings_ui.modal = SettingsModalState::Idle;
    }
    effects.push(ContextEffect::Render.into());
}

pub(super) fn submit_newapi(
    session: &mut AppSession,
    display_name: String,
    base_url: String,
    cookie: String,
    user_id: Option<String>,
    divisor: Option<f64>,
    effects: &mut Vec<AppEffect>,
) {
    let is_editing = matches!(
        session.settings_ui.modal,
        SettingsModalState::EditingNewApi(_)
    );
    let original_filename = session
        .settings_ui
        .modal
        .newapi_edit_data()
        .map(|d| d.original_filename.clone());

    let config = NewApiConfig {
        display_name,
        base_url: base_url.clone(),
        cookie,
        user_id,
        divisor,
    };

    // ── 预注册 Provider ID：确保热重载后 Provider 立即可见 ──
    // 编辑模式下 URL 为只读，所以 ID 不会变化，仅需处理新增场景。
    let new_id = ProviderId::Custom(crate::models::newapi_provider_id(&base_url));
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
            is_editing,
        }
        .into(),
    );
    // SettingsEffect::PersistSettings 和 NotificationEffect::Plain 由 effect handler
    // 在确认写入成功后执行，避免 I/O 失败时产生幽灵 Provider 或虚假通知。
    session.settings_ui.modal = SettingsModalState::Idle;
    effects.push(ContextEffect::Render.into());
}

pub(super) fn edit_newapi(provider_id: ProviderId, effects: &mut Vec<AppEffect>) {
    // 磁盘 I/O 委托给 runtime effect handler，保持 reducer 纯函数
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
    // 先刷新 UI 关闭确认态，避免等待文件删除 / 热重载结果才消失。
    effects.push(ContextEffect::Render.into());
    effects.push(NewApiEffect::DeleteProvider { provider_id }.into());
}

pub(super) fn confirm_delete_newapi(session: &mut AppSession, effects: &mut Vec<AppEffect>) {
    session.settings_ui.modal = SettingsModalState::ConfirmingDeleteNewApi;
    effects.push(ContextEffect::Render.into());
}

pub(super) fn cancel_delete_newapi(session: &mut AppSession, effects: &mut Vec<AppEffect>) {
    if session.settings_ui.modal.is_confirming_delete_newapi() {
        session.settings_ui.modal = SettingsModalState::Idle;
    }
    effects.push(ContextEffect::Render.into());
}
