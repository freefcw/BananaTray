use super::common::{has_effect, has_render, make_custom_provider_status, make_session};
use crate::application::{
    reduce, AppAction, AppEffect, CommonEffect, NewApiEffect, NotificationEffect, RefreshEffect,
    SettingsEffect, SettingsTab,
};
use crate::models::{ProviderId, ProviderKind};
use crate::refresh::{RefreshEvent, RefreshRequest};

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

    // Effect: NewApiEffect::SaveProvider（检查 config 包含关键字段 + 新增模式）
    assert!(has_effect(&effects, |e| {
        matches!(e, AppEffect::Common(CommonEffect::NewApi(NewApiEffect::SaveProvider { config, is_editing, .. }))
            if config.display_name == "Test Site"
            && config.base_url == "https://api.example.com"
            && config.cookie == "session=tok_123"
            && config.user_id == Some("42".to_string())
            && config.divisor == Some(1_000_000.0)
            && !is_editing
        )
    }));

    // SettingsEffect::PersistSettings 和 NotificationEffect::Plain 已移至 runtime 成功路径
    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::PersistSettings))
    )));

    assert!(has_render(&effects));
}

#[test]
fn submit_newapi_auto_enables_and_adds_to_sidebar() {
    let mut session = make_session();
    session.settings_ui.adding_newapi = true;
    // sidebar 初始为空（模拟全新用户场景）
    session.settings.provider.sidebar_providers = vec!["claude".into()];

    let effects = reduce(
        &mut session,
        AppAction::SubmitNewApi {
            display_name: "My Relay".to_string(),
            base_url: "https://relay.example.com".to_string(),
            cookie: "c=1".to_string(),
            user_id: None,
            divisor: None,
        },
    );

    let expected_id = ProviderId::Custom("relay-example-com:newapi".to_string());

    // 自动启用
    assert!(session.settings.provider.is_enabled(&expected_id));
    // 加入 sidebar
    assert!(session
        .settings
        .provider
        .sidebar_providers
        .contains(&"relay-example-com:newapi".to_string()));
    // 设置页选中新 Provider
    assert_eq!(session.settings_ui.selected_provider, expected_id);
    // PersistSettings 已移至 runtime 成功路径，reducer 不再发射
    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::PersistSettings))
    )));
}

#[test]
fn submit_newapi_edit_mode_preserves_existing_enabled_state() {
    use crate::models::NewApiEditData;

    let mut session = make_session();
    let custom_id = ProviderId::Custom("old-site-com:newapi".to_string());
    session.settings.provider.set_enabled(&custom_id, true);
    session
        .settings
        .provider
        .sidebar_providers
        .push("old-site-com:newapi".to_string());

    session.settings_ui.adding_newapi = true;
    session.settings_ui.editing_newapi = Some(NewApiEditData {
        display_name: "Old Site".to_string(),
        base_url: "https://old-site.com".to_string(),
        cookie: "c=old".to_string(),
        user_id: None,
        divisor: None,
        original_filename: "original.yaml".to_string(),
    });

    reduce(
        &mut session,
        AppAction::SubmitNewApi {
            display_name: "Updated Name".to_string(),
            base_url: "https://old-site.com".to_string(), // URL 不变
            cookie: "c=new".to_string(),
            user_id: None,
            divisor: None,
        },
    );

    // 已存在的 enabled 状态不被覆盖
    assert!(session.settings.provider.is_enabled(&custom_id));
}

#[test]
fn submit_newapi_reenables_same_provider_after_create_rollback() {
    let mut session = make_session();
    session.settings_ui.adding_newapi = true;

    let base_url = "https://retry.example.com";
    let retry_id = ProviderId::Custom("retry-example-com:newapi".to_string());

    reduce(
        &mut session,
        AppAction::SubmitNewApi {
            display_name: "Retry Relay".to_string(),
            base_url: base_url.to_string(),
            cookie: "c=1".to_string(),
            user_id: None,
            divisor: None,
        },
    );
    assert!(session.settings.provider.is_enabled(&retry_id));

    crate::application::newapi_ops::rollback_newapi_create(
        &mut session,
        &crate::models::NewApiConfig {
            display_name: "Retry Relay".to_string(),
            base_url: base_url.to_string(),
            cookie: "c=1".to_string(),
            user_id: None,
            divisor: None,
        },
    );
    assert!(!session
        .settings
        .provider
        .enabled_providers
        .contains_key(&retry_id.id_key()));

    reduce(
        &mut session,
        AppAction::SubmitNewApi {
            display_name: "Retry Relay".to_string(),
            base_url: base_url.to_string(),
            cookie: "c=2".to_string(),
            user_id: None,
            divisor: None,
        },
    );

    assert!(session.settings.provider.is_enabled(&retry_id));
    assert!(session
        .settings
        .provider
        .sidebar_providers
        .contains(&retry_id.id_key()));
    assert_eq!(session.settings_ui.selected_provider, retry_id);
}

#[test]
fn providers_reloaded_auto_enables_new_custom_provider() {
    let mut session = make_session();

    // settings 中没有 "fresh:api" 的任何条目
    let mut statuses = session.provider_store.providers.to_vec();
    statuses.push(make_custom_provider_status("fresh:api"));

    let effects = reduce(
        &mut session,
        AppAction::RefreshEventReceived(RefreshEvent::ProvidersReloaded { statuses }),
    );

    let fresh_id = ProviderId::Custom("fresh:api".to_string());
    // 自动启用
    assert!(session.settings.provider.is_enabled(&fresh_id));
    // 加入 sidebar
    assert!(session
        .settings
        .provider
        .sidebar_providers
        .contains(&"fresh:api".to_string()));
    // 产出 PersistSettings
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::PersistSettings))
    )));
    // 触发立即刷新
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Refresh(RefreshEffect::SendRequest(RefreshRequest::RefreshOne {
            ref id,
            ..
        }))) if *id == fresh_id
    )));
}

#[test]
fn providers_reloaded_reuses_existing_sidebar_entry_for_new_custom_provider() {
    let mut session = make_session();
    session
        .settings
        .provider
        .sidebar_providers
        .push("fresh:api".to_string());

    let mut statuses = session.provider_store.providers.to_vec();
    statuses.push(make_custom_provider_status("fresh:api"));

    let effects = reduce(
        &mut session,
        AppAction::RefreshEventReceived(RefreshEvent::ProvidersReloaded { statuses }),
    );

    let fresh_id = ProviderId::Custom("fresh:api".to_string());
    assert!(session.settings.provider.is_enabled(&fresh_id));
    assert_eq!(
        session
            .settings
            .provider
            .sidebar_providers
            .iter()
            .filter(|key| **key == "fresh:api")
            .count(),
        1
    );
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::PersistSettings))
    )));
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
        matches!(e, AppEffect::Common(CommonEffect::NewApi(NewApiEffect::SaveProvider { config, is_editing, .. }))
            if config.base_url == "https://minimal.io"
            && config.divisor.is_none()
            && !is_editing
        )
    }));
}

#[test]
fn select_provider_is_noop_during_newapi_form() {
    // 中转站表单打开时，侧栏点击应完全忽略：
    // 不修改 selected_provider，避免侧栏高亮与表单编辑目标不一致的分叉状态
    let mut session = make_session();
    session.settings_ui.adding_newapi = true;
    let original_selected = session.settings_ui.selected_provider.clone();

    let other_id = session.provider_store.providers[1].provider_id.clone();
    assert_ne!(original_selected, other_id); // 确保测试有意义

    let effects = reduce(&mut session, AppAction::SelectSettingsProvider(other_id));

    assert!(session.settings_ui.adding_newapi); // 表单保留
    assert_eq!(session.settings_ui.selected_provider, original_selected); // 选中不变
    assert!(effects.is_empty()); // 完全 no-op
}

#[test]
fn select_provider_clears_adding_provider() {
    // 添加内置服务商的 picker 是轻量操作，点选已有服务商应退出
    let mut session = make_session();
    session.settings_ui.adding_provider = true;

    let id = session.provider_store.providers[0].provider_id.clone();
    let effects = reduce(&mut session, AppAction::SelectSettingsProvider(id));

    assert!(!session.settings_ui.adding_provider); // picker 已退出
    assert!(has_render(&effects));
}

#[test]
fn set_settings_tab_clears_adding_provider() {
    // 切换 tab 时应退出 picker
    let mut session = make_session();
    session.settings_ui.adding_provider = true;

    let effects = reduce(
        &mut session,
        AppAction::SetSettingsTab(SettingsTab::General),
    );

    assert!(!session.settings_ui.adding_provider);
    assert!(has_render(&effects));
}

#[test]
fn set_settings_tab_preserves_adding_newapi() {
    // 中转站表单是复杂操作，切换 tab 不应丢失表单状态；
    // 用户切回 Providers tab 时应恢复表单界面
    let mut session = make_session();
    session.settings_ui.adding_newapi = true;

    reduce(
        &mut session,
        AppAction::SetSettingsTab(SettingsTab::General),
    );

    assert!(session.settings_ui.adding_newapi); // 表单状态保留
}

// ── 编辑模式 ──────────────────────────────────────

#[test]
fn submit_newapi_in_edit_mode_uses_original_filename() {
    use crate::models::NewApiEditData;

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

    // Effect: 使用原始文件名 + 编辑模式标志
    assert!(has_effect(&effects, |e| {
        matches!(e, AppEffect::Common(CommonEffect::NewApi(NewApiEffect::SaveProvider { config, original_filename, is_editing }))
            if *original_filename == Some("original-file.yaml".to_string())
            && config.display_name == "Updated Name"
            && config.cookie == "new_cookie"
            && *is_editing
        )
    }));

    // SettingsEffect::PersistSettings 和通知已移至 runtime 成功路径
    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Notification(NotificationEffect::Plain { .. }))
    )));
}

#[test]
fn cancel_add_newapi_clears_editing_state() {
    use crate::models::NewApiEditData;

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
    use crate::models::NewApiEditData;

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

// ── DeleteNewApi ──────────────────────────────────────────────────────────

#[test]
fn delete_newapi_produces_delete_effect_with_correct_provider_id() {
    let mut session = make_session();
    let id = ProviderId::Custom("my-api-example-com:newapi".to_string());
    session.settings_ui.confirming_delete_newapi = true;

    let effects = reduce(
        &mut session,
        AppAction::DeleteNewApi {
            provider_id: id.clone(),
        },
    );

    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::NewApi(NewApiEffect::DeleteProvider { provider_id }))
            if *provider_id == id
    )));
    assert!(!session.settings_ui.confirming_delete_newapi);
    assert!(has_render(&effects));
}

#[test]
fn delete_newapi_emits_effect_for_any_provider_id() {
    // 文件名推导和 `:newapi` 检查已移至 runtime，reducer 统一发射
    let mut session = make_session();
    let id = ProviderId::Custom("some-other-provider:cli".to_string());

    let effects = reduce(
        &mut session,
        AppAction::DeleteNewApi {
            provider_id: id.clone(),
        },
    );

    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::NewApi(NewApiEffect::DeleteProvider { provider_id }))
            if *provider_id == id
    )));
}

#[test]
fn delete_newapi_emits_effect_for_builtin_provider() {
    let mut session = make_session();
    let id = ProviderId::BuiltIn(ProviderKind::Claude);

    let effects = reduce(&mut session, AppAction::DeleteNewApi { provider_id: id });

    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::NewApi(NewApiEffect::DeleteProvider { .. }))
    )));
}

#[test]
fn confirm_delete_newapi_sets_confirming_flag() {
    let mut session = make_session();
    assert!(!session.settings_ui.confirming_delete_newapi);

    let effects = reduce(&mut session, AppAction::ConfirmDeleteNewApi);

    assert!(session.settings_ui.confirming_delete_newapi);
    assert!(has_render(&effects));
}

#[test]
fn cancel_delete_newapi_clears_confirming_flag() {
    let mut session = make_session();
    session.settings_ui.confirming_delete_newapi = true;

    let effects = reduce(&mut session, AppAction::CancelDeleteNewApi);

    assert!(!session.settings_ui.confirming_delete_newapi);
    assert!(has_render(&effects));
}

#[test]
fn enter_add_newapi_clears_adding_provider() {
    let mut session = make_session();
    session.settings_ui.adding_provider = true;

    let effects = reduce(&mut session, AppAction::EnterAddNewApi);

    assert!(session.settings_ui.adding_newapi);
    assert!(!session.settings_ui.adding_provider); // 互斥清除
    assert!(has_render(&effects));
}
