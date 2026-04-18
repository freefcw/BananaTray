//! NewAPI Provider 保存操作的纯函数辅助模块。
//!
//! 从 `runtime/mod.rs` 的 `SaveNewApiProvider` handler 中提取的状态操作逻辑，
//! 包括保存失败时的回滚和通知 i18n key 选择。
//!
//! 本模块为纯函数，不包含 I/O 或 GPUI 依赖，可通过 `cargo test --lib` 测试。

use super::state::AppSession;
use crate::models::{NewApiConfig, NewApiEditData, ProviderId, ProviderKind};

/// 编辑模式下的回滚：恢复表单编辑状态，让用户可以重试。
///
/// 编辑模式时旧 YAML 文件仍在磁盘上，不需要回滚 enabled/sidebar 预注册。
/// 仅需从 config 重建 `NewApiEditData` 回填表单。
pub fn rollback_newapi_edit(session: &mut AppSession, config: &NewApiConfig, filename: &str) {
    session.settings_ui.adding_newapi = true;
    session.settings_ui.editing_newapi = Some(NewApiEditData {
        display_name: config.display_name.clone(),
        base_url: config.base_url.clone(),
        cookie: config.cookie.clone(),
        user_id: config.user_id.clone(),
        divisor: config.divisor,
        original_filename: filename.to_string(),
    });
}

/// 新增模式下的回滚：撤销 reducer 预注册的 provider ID 并恢复空表单。
///
/// 回滚内容：
/// 1. 将预注册的 provider ID 从 enabled + sidebar 中移除
/// 2. 重新打开空的添加表单
/// 3. 恢复 `selected_provider` 到 sidebar 的第一项
pub fn rollback_newapi_create(session: &mut AppSession, config: &NewApiConfig) {
    let rollback_id = ProviderId::Custom(crate::models::newapi_provider_id(&config.base_url));
    session
        .settings
        .provider
        .remove_enabled_record(&rollback_id);
    session.settings.provider.remove_from_sidebar(&rollback_id);

    // 重新打开空表单让用户可以重试
    session.settings_ui.adding_newapi = true;
    session.settings_ui.editing_newapi = None;

    // 恢复 selected_provider 到 sidebar 第一项
    let custom_ids = session.provider_store.custom_provider_ids();
    let sidebar_ids = session.settings.provider.sidebar_provider_ids(&custom_ids);
    session.settings_ui.selected_provider = sidebar_ids
        .first()
        .cloned()
        .unwrap_or(ProviderId::BuiltIn(ProviderKind::Claude));
}

/// 根据保存结果选择通知的 i18n key 对。
///
/// 三种场景：
/// 1. YAML 成功但 settings 持久化失败 → partial 提示
/// 2. 编辑模式完全成功 → edit_success
/// 3. 新增模式完全成功 → save_success
pub fn newapi_save_notification_keys(
    is_editing: bool,
    settings_saved: bool,
) -> (&'static str, &'static str) {
    if !settings_saved {
        ("newapi.save_partial_title", "newapi.save_partial_body")
    } else if is_editing {
        ("newapi.edit_success_title", "newapi.edit_success_body")
    } else {
        ("newapi.save_success_title", "newapi.save_success_body")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::AppSettings;

    fn make_session() -> AppSession {
        AppSession::new(AppSettings::default(), vec![])
    }

    fn make_config() -> NewApiConfig {
        NewApiConfig {
            display_name: "Test API".to_string(),
            base_url: "https://my-api.example.com".to_string(),
            cookie: "session=abc123".to_string(),
            user_id: Some("42".to_string()),
            divisor: Some(500_000.0),
        }
    }

    // ── rollback_newapi_edit ──────────────────────────

    #[test]
    fn rollback_edit_restores_form_with_config_data() {
        let mut session = make_session();
        let config = make_config();

        rollback_newapi_edit(&mut session, &config, "newapi-test.yaml");

        assert!(session.settings_ui.adding_newapi);
        let edit = session.settings_ui.editing_newapi.as_ref().unwrap();
        assert_eq!(edit.display_name, "Test API");
        assert_eq!(edit.base_url, "https://my-api.example.com");
        assert_eq!(edit.cookie, "session=abc123");
        assert_eq!(edit.user_id.as_deref(), Some("42"));
        assert_eq!(edit.divisor, Some(500_000.0));
        assert_eq!(edit.original_filename, "newapi-test.yaml");
    }

    // ── rollback_newapi_create ────────────────────────

    #[test]
    fn rollback_create_removes_pre_registered_provider() {
        let mut session = make_session();
        let config = make_config();

        // 模拟 reducer 预注册
        let pre_id = ProviderId::Custom(crate::models::newapi_provider_id(&config.base_url));
        session.settings.provider.set_enabled(&pre_id, true);
        session.settings.provider.add_to_sidebar(&pre_id);
        session.settings_ui.selected_provider = pre_id.clone();

        rollback_newapi_create(&mut session, &config);

        // 验证预注册已回滚
        assert!(!session.settings.provider.is_enabled(&pre_id));
        assert!(!session
            .settings
            .provider
            .enabled_providers
            .contains_key(&pre_id.id_key()));
        assert!(!session
            .settings
            .provider
            .sidebar_provider_ids(&[])
            .contains(&pre_id));

        // 验证表单恢复为新增模式
        assert!(session.settings_ui.adding_newapi);
        assert!(session.settings_ui.editing_newapi.is_none());
    }

    #[test]
    fn rollback_create_restores_selected_provider_to_first_sidebar_item() {
        let mut session = make_session();
        let config = make_config();

        // 预置一个已有的 sidebar provider
        let existing_id = ProviderId::BuiltIn(ProviderKind::Claude);
        session.settings.provider.add_to_sidebar(&existing_id);
        session.settings.provider.set_enabled(&existing_id, true);

        // 模拟 reducer 预注册
        let pre_id = ProviderId::Custom(crate::models::newapi_provider_id(&config.base_url));
        session.settings.provider.set_enabled(&pre_id, true);
        session.settings.provider.add_to_sidebar(&pre_id);
        session.settings_ui.selected_provider = pre_id.clone();

        rollback_newapi_create(&mut session, &config);

        // selected_provider 应恢复到 sidebar 第一项
        assert_eq!(session.settings_ui.selected_provider, existing_id);
    }

    // ── newapi_save_notification_keys ─────────────────

    #[test]
    fn notification_keys_partial_when_settings_not_saved() {
        let (title, body) = newapi_save_notification_keys(false, false);
        assert_eq!(title, "newapi.save_partial_title");
        assert_eq!(body, "newapi.save_partial_body");

        // 即使 is_editing = true，settings 未保存也应该返回 partial
        let (title, body) = newapi_save_notification_keys(true, false);
        assert_eq!(title, "newapi.save_partial_title");
        assert_eq!(body, "newapi.save_partial_body");
    }

    #[test]
    fn notification_keys_edit_success() {
        let (title, body) = newapi_save_notification_keys(true, true);
        assert_eq!(title, "newapi.edit_success_title");
        assert_eq!(body, "newapi.edit_success_body");
    }

    #[test]
    fn notification_keys_new_save_success() {
        let (title, body) = newapi_save_notification_keys(false, true);
        assert_eq!(title, "newapi.save_success_title");
        assert_eq!(body, "newapi.save_success_body");
    }
}
