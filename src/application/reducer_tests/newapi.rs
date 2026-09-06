use super::common::{has_effect, has_render, make_custom_provider_status, make_session};
use crate::application::{
    reduce, AppAction, AppEffect, CommonEffect, NewApiEffect, NotificationEffect, RefreshEffect,
    SettingChange, SettingsEffect, SettingsModalState, SettingsTab,
};
use crate::models::{
    CustomProviderLifecycleFailure, NewApiConfig, NewApiEditData, NewApiSaveSuccess, ProviderId,
    ProviderKind,
};
use crate::refresh::{RefreshEvent, RefreshRequest};

fn make_newapi_config() -> NewApiConfig {
    NewApiConfig {
        display_name: "Relay".to_string(),
        base_url: "https://relay.example.com".to_string(),
        cookie: "session=abc".to_string(),
        user_id: None,
        divisor: None,
    }
}

// ── NewAPI 快速添加 ────────────────────────────────

#[test]
fn edit_newapi_clears_token_editing_state() {
    let mut session = make_session();
    session.settings_ui.token_editing_provider = Some(ProviderId::BuiltIn(ProviderKind::Copilot));
    let id = ProviderId::Custom("ccswitch:newapi".to_string());

    let effects = reduce(
        &mut session,
        AppAction::EditNewApi {
            provider_id: id.clone(),
        },
    );

    assert!(session.settings_ui.token_editing_provider.is_none());
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::NewApi(NewApiEffect::LoadConfig { provider_id }))
            if *provider_id == id
    )));
    assert!(has_render(&effects));
}

#[test]
fn enter_add_newapi_sets_flag_true() {
    let mut session = make_session();
    assert!(!session.settings_ui.modal.is_newapi_form());
    session.settings_ui.token_editing_provider = Some(ProviderId::BuiltIn(ProviderKind::Copilot));

    let effects = reduce(&mut session, AppAction::EnterAddNewApi);

    assert_eq!(session.settings_ui.modal, SettingsModalState::AddingNewApi);
    assert!(session.settings_ui.token_editing_provider.is_none());
    assert!(has_render(&effects));
}

#[test]
fn cancel_add_newapi_resets_flag() {
    let mut session = make_session();
    session.settings_ui.modal = SettingsModalState::AddingNewApi;
    session.settings_ui.token_editing_provider = Some(ProviderId::BuiltIn(ProviderKind::Copilot));

    let effects = reduce(&mut session, AppAction::CancelAddNewApi);

    assert!(!session.settings_ui.modal.is_newapi_form());
    assert!(session.settings_ui.token_editing_provider.is_none());
    assert!(has_render(&effects));
}

#[test]
fn submit_newapi_produces_save_and_notification_effects() {
    let mut session = make_session();
    session.settings_ui.modal = SettingsModalState::AddingNewApi;

    let effects = reduce(
        &mut session,
        AppAction::SubmitNewApi(NewApiConfig {
            display_name: "Test Site".to_string(),
            base_url: "https://api.example.com".to_string(),
            cookie: "session=tok_123".to_string(),
            user_id: Some("42".to_string()),
            divisor: Some(1_000_000.0),
        }),
    );

    // 状态：表单已关闭
    assert!(!session.settings_ui.modal.is_newapi_form());

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

    // Submit 只发起保存 I/O；持久化 flush 和通知由完成事件链路处理
    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::PersistSettings))
    )));

    assert!(has_render(&effects));
}

#[test]
fn submit_newapi_duplicate_identity_is_rejected() {
    let mut session = make_session();
    session.settings_ui.modal = SettingsModalState::AddingNewApi;

    // 同站点同账号已有启用记录（即使被禁用也视为占用，防止静默覆盖 YAML）
    let existing = ProviderId::Custom("api-example-com:newapi".to_string());
    session.settings.provider.set_enabled(&existing, false);

    let effects = reduce(
        &mut session,
        AppAction::SubmitNewApi(NewApiConfig {
            display_name: "Dup".to_string(),
            base_url: "https://api.example.com".to_string(),
            cookie: "session=x".to_string(),
            user_id: None,
            divisor: None,
        }),
    );

    // 不发起保存；表单保持打开；提示冲突
    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::NewApi(NewApiEffect::SaveProvider { .. }))
    )));
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Notification(NotificationEffect::PlainI18n {
            title_key: "newapi.duplicate_title",
            body_key: "newapi.duplicate_body",
        }))
    )));
    assert_eq!(session.settings_ui.modal, SettingsModalState::AddingNewApi);
}

#[test]
fn submit_newapi_duplicate_identity_from_loaded_provider_is_rejected() {
    let mut session = make_session();
    session.settings_ui.modal = SettingsModalState::AddingNewApi;

    // 磁盘已加载的 provider（settings 中无记录）同样视为占用
    session
        .provider_store
        .providers
        .push(make_custom_provider_status("api-example-com-7:newapi"));

    let effects = reduce(
        &mut session,
        AppAction::SubmitNewApi(NewApiConfig {
            display_name: "Dup".to_string(),
            base_url: "https://api.example.com".to_string(),
            cookie: "session=x".to_string(),
            user_id: Some("7".to_string()),
            divisor: None,
        }),
    );

    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::NewApi(NewApiEffect::SaveProvider { .. }))
    )));
    assert_eq!(session.settings_ui.modal, SettingsModalState::AddingNewApi);
}

#[test]
fn submit_newapi_same_site_different_user_id_allowed() {
    let mut session = make_session();
    session.settings_ui.modal = SettingsModalState::AddingNewApi;

    // 已占用的是无 user_id 身份；带 user_id 的提交是同站多账号，应放行
    let existing = ProviderId::Custom("api-example-com:newapi".to_string());
    session.settings.provider.set_enabled(&existing, true);

    let effects = reduce(
        &mut session,
        AppAction::SubmitNewApi(NewApiConfig {
            display_name: "Second Account".to_string(),
            base_url: "https://api.example.com".to_string(),
            cookie: "session=y".to_string(),
            user_id: Some("7".to_string()),
            divisor: None,
        }),
    );

    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::NewApi(NewApiEffect::SaveProvider { .. }))
    )));
    let new_id = ProviderId::Custom("api-example-com-7:newapi".to_string());
    assert!(session.settings.provider.is_enabled(&new_id));
    assert_eq!(session.settings_ui.selected_provider, new_id);
}

#[test]
fn newapi_save_finished_success_notifies_and_reloads_providers() {
    let mut session = make_session();
    let effects = reduce(
        &mut session,
        AppAction::NewApiSaveFinished {
            request_id: 0,
            config: make_newapi_config(),
            filename: "newapi-relay.yaml".to_string(),
            original_id: None,
            is_editing: false,
            result: Ok(NewApiSaveSuccess {
                path: std::path::PathBuf::from("newapi-relay.yaml"),
                settings_saved: true,
            }),
        },
    );

    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Notification(NotificationEffect::PlainI18n {
            title_key: "newapi.save_success_title",
            body_key: "newapi.save_success_body",
        }))
    )));
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Refresh(RefreshEffect::SendRequest(
            RefreshRequest::ReloadProviders
        )))
    )));
}

#[test]
fn newapi_save_finished_failure_rolls_back_create_and_notifies() {
    let mut session = make_session();
    let config = make_newapi_config();
    let provider_id = ProviderId::Custom(crate::models::newapi_provider_id(
        &config.base_url,
        config.user_id.as_deref(),
    ));
    let provider_key = provider_id.id_key();
    session.settings.provider.set_enabled(&provider_id, true);
    session.settings.provider.add_to_sidebar(&provider_id);
    session.settings.provider.hidden_quotas.insert(
        provider_key.clone(),
        ["Session".to_string()].into_iter().collect(),
    );
    session.settings_ui.selected_provider = provider_id.clone();
    let request_id = session.settings_ui.begin_custom_provider_save();

    let effects = reduce(
        &mut session,
        AppAction::NewApiSaveFinished {
            request_id,
            config,
            filename: "newapi-relay.yaml".to_string(),
            original_id: None,
            is_editing: false,
            result: Err(CustomProviderLifecycleFailure::file_operation(
                "save NewAPI provider",
                "disk full",
            )),
        },
    );

    assert!(!session.settings.provider.is_enabled(&provider_id));
    assert!(!session.settings.provider.has_layout_item(&provider_id));
    assert!(!session
        .settings
        .provider
        .hidden_quotas
        .contains_key(&provider_key));
    assert!(session.settings_ui.modal.is_newapi_form());
    assert!(has_render(&effects));
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Notification(NotificationEffect::PlainI18n {
            title_key: "newapi.save_failed_title",
            body_key: "newapi.save_failed_body",
        }))
    )));
}

#[test]
fn newapi_save_failure_persists_rollback_after_newer_settings_write_is_scheduled() {
    let mut session = make_session();
    session.settings_ui.modal = SettingsModalState::AddingNewApi;
    let config = make_newapi_config();
    let provider_id = ProviderId::Custom(crate::models::newapi_provider_id(
        &config.base_url,
        config.user_id.as_deref(),
    ));

    reduce(&mut session, AppAction::SubmitNewApi(config.clone()));
    let request_id = session
        .settings_ui
        .pending_custom_provider_save_request_id
        .expect("save request id");
    assert!(session.settings.provider.is_enabled(&provider_id));

    let newer_settings_effects = reduce(
        &mut session,
        AppAction::UpdateSetting(SettingChange::ToggleShowDashboardButton),
    );
    assert!(has_effect(&newer_settings_effects, |effect| matches!(
        effect,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::PersistSettings))
    )));

    let effects = reduce(
        &mut session,
        AppAction::NewApiSaveFinished {
            request_id,
            config,
            filename: "newapi-relay.yaml".to_string(),
            original_id: None,
            is_editing: false,
            result: Err(CustomProviderLifecycleFailure::file_operation(
                "save NewAPI provider",
                "disk full",
            )),
        },
    );

    assert!(!session.settings.provider.has_layout_item(&provider_id));
    assert!(has_effect(&effects, |effect| matches!(
        effect,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::PersistSettings))
    )));
}

#[test]
fn late_newapi_save_failure_preserves_the_users_new_form_context() {
    let mut session = make_session();
    session.settings_ui.modal = SettingsModalState::AddingNewApi;
    let config = make_newapi_config();
    let failed_id = ProviderId::Custom(crate::models::newapi_provider_id(
        &config.base_url,
        config.user_id.as_deref(),
    ));
    reduce(&mut session, AppAction::SubmitNewApi(config.clone()));
    let request_id = session
        .settings_ui
        .pending_custom_provider_save_request_id
        .expect("save request id");

    let later_selected = ProviderId::BuiltIn(ProviderKind::Gemini);
    reduce(
        &mut session,
        AppAction::SelectSettingsProvider(later_selected.clone()),
    );
    reduce(&mut session, AppAction::EnterAddScriptProvider);

    reduce(
        &mut session,
        AppAction::NewApiSaveFinished {
            request_id,
            config,
            filename: "newapi-relay.yaml".to_string(),
            original_id: None,
            is_editing: false,
            result: Err(CustomProviderLifecycleFailure::file_operation(
                "save NewAPI provider",
                "disk full",
            )),
        },
    );

    assert!(!session.settings.provider.has_layout_item(&failed_id));
    assert_eq!(
        session.settings_ui.modal,
        SettingsModalState::AddingScriptProvider
    );
    assert_eq!(session.settings_ui.selected_provider, later_selected);
}

#[test]
fn newapi_delete_finished_success_reloads_providers() {
    let mut session = make_session();
    session
        .settings_ui
        .begin_custom_provider_delete(ProviderId::Custom("relay:newapi".to_string()));
    let provider_id = ProviderId::Custom("relay:newapi".to_string());
    let key = provider_id.id_key();
    session.settings.provider.set_enabled(&provider_id, true);
    session.settings.provider.add_to_sidebar(&provider_id);
    session
        .settings
        .provider
        .hidden_quotas
        .insert(key.clone(), ["Daily".to_string()].into_iter().collect());
    let effects = reduce(
        &mut session,
        AppAction::NewApiDeleteFinished {
            request_id: 1,
            provider_id,
            result: Ok(std::path::PathBuf::from("newapi-relay.yaml")),
        },
    );

    assert!(!session
        .settings
        .provider
        .has_layout_item(&ProviderId::Custom(key.clone())));
    assert!(!session.settings.provider.hidden_quotas.contains_key(&key));
    assert!(has_effect(&effects, |effect| matches!(
        effect,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::PersistSettings))
    )));
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Refresh(RefreshEffect::SendRequest(
            RefreshRequest::ReloadProviders
        )))
    )));
}

#[test]
fn newapi_delete_finished_failure_notifies_without_reload() {
    let mut session = make_session();
    session
        .settings_ui
        .begin_custom_provider_delete(ProviderId::Custom("missing:newapi".to_string()));
    let effects = reduce(
        &mut session,
        AppAction::NewApiDeleteFinished {
            request_id: 1,
            provider_id: ProviderId::Custom("missing:newapi".to_string()),
            result: Err(CustomProviderLifecycleFailure::yaml_not_found(
                "delete NewAPI provider",
                "missing:newapi",
                None,
            )),
        },
    );

    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Notification(NotificationEffect::PlainI18n {
            title_key: "newapi.delete_failed_title",
            body_key: "newapi.delete_failed_body",
        }))
    )));
    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Refresh(RefreshEffect::SendRequest(
            RefreshRequest::ReloadProviders
        )))
    )));
}

#[test]
fn newapi_load_finished_failure_notifies_and_renders() {
    let mut session = make_session();
    let provider_id = ProviderId::Custom("missing:newapi".to_string());
    reduce(
        &mut session,
        AppAction::EditNewApi {
            provider_id: provider_id.clone(),
        },
    );
    let effects = reduce(
        &mut session,
        AppAction::NewApiLoadFinished {
            provider_id,
            result: Err(CustomProviderLifecycleFailure::yaml_not_found(
                "load NewAPI provider",
                "missing:newapi",
                None,
            )),
        },
    );

    assert!(has_render(&effects));
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Notification(NotificationEffect::PlainI18n {
            title_key: "newapi.load_failed_title",
            body_key: "newapi.load_failed_body",
        }))
    )));
}

#[test]
fn newapi_load_finished_success_sets_edit_modal() {
    let mut session = make_session();
    let provider_id = ProviderId::Custom("relay:newapi".to_string());
    reduce(
        &mut session,
        AppAction::EditNewApi {
            provider_id: provider_id.clone(),
        },
    );
    let edit_data = NewApiEditData {
        display_name: "Relay".to_string(),
        base_url: "https://relay.example.com".to_string(),
        cookie: "c=1".to_string(),
        user_id: None,
        divisor: None,
        original_filename: "newapi-relay.yaml".to_string(),
        original_id: "relay-example-com:newapi".to_string(),
    };

    let effects = reduce(
        &mut session,
        AppAction::NewApiLoadFinished {
            provider_id,
            result: Ok(edit_data.clone()),
        },
    );

    assert_eq!(
        session.settings_ui.modal,
        SettingsModalState::EditingNewApi(edit_data)
    );
    assert!(has_render(&effects));
}

#[test]
fn late_newapi_load_does_not_replace_a_newer_modal() {
    let mut session = make_session();
    let provider_id = ProviderId::Custom("relay:newapi".to_string());
    reduce(
        &mut session,
        AppAction::EditNewApi {
            provider_id: provider_id.clone(),
        },
    );
    reduce(&mut session, AppAction::EnterAddProvider);

    let effects = reduce(
        &mut session,
        AppAction::NewApiLoadFinished {
            provider_id,
            result: Ok(NewApiEditData {
                display_name: "Relay".to_string(),
                base_url: "https://relay.example.com".to_string(),
                cookie: "c=1".to_string(),
                user_id: None,
                divisor: None,
                original_filename: "newapi-relay.yaml".to_string(),
                original_id: "relay-example-com:newapi".to_string(),
            }),
        },
    );

    assert_eq!(
        session.settings_ui.modal,
        SettingsModalState::AddingProvider
    );
    assert!(effects.is_empty(), "stale completion should be ignored");
}

#[test]
fn submit_newapi_auto_enables_and_adds_to_sidebar() {
    let mut session = make_session();
    session.settings_ui.modal = SettingsModalState::AddingNewApi;
    let effects = reduce(
        &mut session,
        AppAction::SubmitNewApi(NewApiConfig {
            display_name: "My Relay".to_string(),
            base_url: "https://relay.example.com".to_string(),
            cookie: "c=1".to_string(),
            user_id: None,
            divisor: None,
        }),
    );

    let expected_id = ProviderId::Custom("relay-example-com:newapi".to_string());

    // 自动启用
    assert!(session.settings.provider.is_enabled(&expected_id));
    // 加入 sidebar
    assert!(session.settings.provider.is_in_sidebar(&expected_id));
    // 设置页选中新 Provider
    assert_eq!(session.settings_ui.selected_provider, expected_id);
    // Submit 阶段不发 PersistSettings；保存完成后再按结果处理
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
    session.settings.provider.add_to_sidebar(&custom_id);

    session.settings_ui.modal = SettingsModalState::EditingNewApi(NewApiEditData {
        display_name: "Old Site".to_string(),
        base_url: "https://old-site.com".to_string(),
        cookie: "c=old".to_string(),
        user_id: None,
        divisor: None,
        original_filename: "original.yaml".to_string(),
        original_id: "old-site-com:newapi".to_string(),
    });

    reduce(
        &mut session,
        AppAction::SubmitNewApi(NewApiConfig {
            display_name: "Updated Name".to_string(),
            base_url: "https://old-site.com".to_string(), // URL 不变
            cookie: "c=new".to_string(),
            user_id: None,
            divisor: None,
        }),
    );

    // 已存在的 enabled 状态不被覆盖，也不会重复加入 sidebar
    assert!(session.settings.provider.is_enabled(&custom_id));
    assert!(session.settings.provider.is_in_sidebar(&custom_id));
}

#[test]
fn submit_newapi_reenables_same_provider_after_create_rollback() {
    let mut session = make_session();
    session.settings_ui.modal = SettingsModalState::AddingNewApi;

    let base_url = "https://retry.example.com";
    let retry_id = ProviderId::Custom("retry-example-com:newapi".to_string());

    reduce(
        &mut session,
        AppAction::SubmitNewApi(NewApiConfig {
            display_name: "Retry Relay".to_string(),
            base_url: base_url.to_string(),
            cookie: "c=1".to_string(),
            user_id: None,
            divisor: None,
        }),
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
    assert!(!session.settings.provider.has_layout_item(&retry_id));

    reduce(
        &mut session,
        AppAction::SubmitNewApi(NewApiConfig {
            display_name: "Retry Relay".to_string(),
            base_url: base_url.to_string(),
            cookie: "c=2".to_string(),
            user_id: None,
            divisor: None,
        }),
    );

    assert!(session.settings.provider.is_enabled(&retry_id));
    assert!(session.settings.provider.is_in_sidebar(&retry_id));
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
    assert!(session.settings.provider.is_in_sidebar(&fresh_id));
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
    let fresh_id = ProviderId::Custom("fresh:api".to_string());
    session.settings.provider.set_enabled(&fresh_id, true);
    session.settings.provider.add_to_sidebar(&fresh_id);

    let mut statuses = session.provider_store.providers.to_vec();
    statuses.push(make_custom_provider_status("fresh:api"));

    let effects = reduce(
        &mut session,
        AppAction::RefreshEventReceived(RefreshEvent::ProvidersReloaded { statuses }),
    );

    assert!(session.settings.provider.is_enabled(&fresh_id));
    assert!(session.settings.provider.is_in_sidebar(&fresh_id));
    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::PersistSettings))
    )));
}

#[test]
fn submit_newapi_without_optional_fields_uses_defaults() {
    let mut session = make_session();

    let effects = reduce(
        &mut session,
        AppAction::SubmitNewApi(NewApiConfig {
            display_name: "Minimal".to_string(),
            base_url: "https://minimal.io".to_string(),
            cookie: "session=abc".to_string(),
            user_id: None,
            divisor: None,
        }),
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
    session.settings_ui.modal = SettingsModalState::AddingNewApi;
    let original_selected = session.settings_ui.selected_provider.clone();

    let other_id = session.provider_store.providers[1].provider_id.clone();
    assert_ne!(original_selected, other_id); // 确保测试有意义

    let effects = reduce(&mut session, AppAction::SelectSettingsProvider(other_id));

    assert!(session.settings_ui.modal.is_newapi_form()); // 表单保留
    assert_eq!(session.settings_ui.selected_provider, original_selected); // 选中不变
    assert!(effects.is_empty()); // 完全 no-op
}

#[test]
fn select_provider_clears_adding_provider() {
    // 添加内置服务商的 picker 是轻量操作，点选已有服务商应退出
    let mut session = make_session();
    session.settings_ui.modal = SettingsModalState::AddingProvider;

    let id = session.provider_store.providers[0].provider_id.clone();
    let effects = reduce(&mut session, AppAction::SelectSettingsProvider(id));

    assert!(!session.settings_ui.modal.is_adding_provider()); // picker 已退出
    assert!(has_render(&effects));
}

#[test]
fn set_settings_tab_clears_adding_provider() {
    // 切换 tab 时应退出 picker
    let mut session = make_session();
    session.settings_ui.modal = SettingsModalState::AddingProvider;
    session.settings_ui.token_editing_provider = Some(ProviderId::BuiltIn(ProviderKind::Copilot));

    let effects = reduce(
        &mut session,
        AppAction::SetSettingsTab(SettingsTab::General),
    );

    assert!(!session.settings_ui.modal.is_adding_provider());
    assert!(session.settings_ui.token_editing_provider.is_none());
    assert!(has_render(&effects));
}

#[test]
fn set_settings_tab_preserves_adding_newapi() {
    // 中转站表单是复杂操作，切换 tab 不应丢失表单状态；
    // 用户切回 Providers tab 时应恢复表单界面
    let mut session = make_session();
    session.settings_ui.modal = SettingsModalState::AddingNewApi;

    reduce(
        &mut session,
        AppAction::SetSettingsTab(SettingsTab::General),
    );

    assert!(session.settings_ui.modal.is_newapi_form()); // 表单状态保留
}

// ── 编辑模式 ──────────────────────────────────────

#[test]
fn submit_newapi_in_edit_mode_uses_original_filename() {
    use crate::models::NewApiEditData;

    let mut session = make_session();
    session.settings_ui.modal = SettingsModalState::EditingNewApi(NewApiEditData {
        display_name: "Old Name".to_string(),
        base_url: "https://old-site.com".to_string(),
        cookie: "old_cookie".to_string(),
        user_id: None,
        divisor: None,
        original_filename: "original-file.yaml".to_string(),
        original_id: "old-site-com:newapi".to_string(),
    });

    let effects = reduce(
        &mut session,
        AppAction::SubmitNewApi(NewApiConfig {
            display_name: "Updated Name".to_string(),
            base_url: "https://old-site.com".to_string(),
            cookie: "new_cookie".to_string(),
            user_id: Some("99".to_string()),
            divisor: Some(1_000_000.0),
        }),
    );

    // 状态：编辑模式已清除（modal 回到 Idle）
    assert_eq!(session.settings_ui.modal, SettingsModalState::Idle);

    // Effect: 使用原始文件名 + 原始身份 + 编辑模式标志
    // （user_id 从 None 改为 Some("99")，身份仍保持 original_id 不变）
    assert!(has_effect(&effects, |e| {
        matches!(e, AppEffect::Common(CommonEffect::NewApi(NewApiEffect::SaveProvider { config, original_filename, original_id, is_editing, .. }))
            if *original_filename == Some("original-file.yaml".to_string())
            && *original_id == Some("old-site-com:newapi".to_string())
            && config.display_name == "Updated Name"
            && config.cookie == "new_cookie"
            && *is_editing
        )
    }));

    // Submit 阶段不发通知；保存完成后由 NewApiSaveFinished 按结果发通知
    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Notification(..))
    )));
}

#[test]
fn submit_newapi_in_edit_mode_keeps_original_base_url_identity() {
    use crate::models::NewApiEditData;

    let mut session = make_session();
    let original_id = ProviderId::Custom("old-site-com:newapi".to_string());
    let payload_id = ProviderId::Custom("changed-site-com:newapi".to_string());
    session.settings.provider.set_enabled(&original_id, true);
    session.settings_ui.modal = SettingsModalState::EditingNewApi(NewApiEditData {
        display_name: "Old Name".to_string(),
        base_url: "https://old-site.com".to_string(),
        cookie: "old_cookie".to_string(),
        user_id: None,
        divisor: None,
        original_filename: "original-file.yaml".to_string(),
        original_id: "old-site-com:newapi".to_string(),
    });

    let effects = reduce(
        &mut session,
        AppAction::SubmitNewApi(NewApiConfig {
            display_name: "Updated Name".to_string(),
            base_url: "https://changed-site.com".to_string(),
            cookie: "new_cookie".to_string(),
            user_id: None,
            divisor: None,
        }),
    );

    assert!(session.settings.provider.is_enabled(&original_id));
    assert!(!session.settings.provider.is_enabled(&payload_id));
    assert_eq!(session.settings_ui.selected_provider, original_id);
    assert!(has_effect(&effects, |e| {
        matches!(e, AppEffect::Common(CommonEffect::NewApi(NewApiEffect::SaveProvider { config, original_filename, is_editing, .. }))
            if config.base_url == "https://old-site.com"
            && *original_filename == Some("original-file.yaml".to_string())
            && *is_editing
        )
    }));
}

#[test]
fn cancel_add_newapi_clears_editing_state() {
    use crate::models::NewApiEditData;

    let mut session = make_session();
    session.settings_ui.modal = SettingsModalState::EditingNewApi(NewApiEditData {
        display_name: "Test".to_string(),
        base_url: "https://test.com".to_string(),
        cookie: "c".to_string(),
        user_id: None,
        divisor: None,
        original_filename: "test.yaml".to_string(),
        original_id: "test-com:newapi".to_string(),
    });

    let effects = reduce(&mut session, AppAction::CancelAddNewApi);

    assert_eq!(session.settings_ui.modal, SettingsModalState::Idle);
    assert!(has_render(&effects));
}

#[test]
fn enter_add_newapi_clears_stale_editing_state() {
    use crate::models::NewApiEditData;

    let mut session = make_session();
    // 模拟残留的编辑状态
    session.settings_ui.modal = SettingsModalState::EditingNewApi(NewApiEditData {
        display_name: "Stale".to_string(),
        base_url: "https://stale.com".to_string(),
        cookie: "c".to_string(),
        user_id: None,
        divisor: None,
        original_filename: "stale.yaml".to_string(),
        original_id: "stale-com:newapi".to_string(),
    });

    let effects = reduce(&mut session, AppAction::EnterAddNewApi);

    // 进入纯新增模式：modal 切到 AddingNewApi，回填数据被丢弃
    assert_eq!(session.settings_ui.modal, SettingsModalState::AddingNewApi);
    assert!(has_render(&effects));
}

// ── DeleteNewApi ──────────────────────────────────────────────────────────

#[test]
fn delete_newapi_produces_delete_effect_with_correct_provider_id() {
    let mut session = make_session();
    let id = ProviderId::Custom("my-api-example-com:newapi".to_string());
    session.settings_ui.modal = SettingsModalState::ConfirmingDeleteNewApi;
    session.settings_ui.token_editing_provider = Some(ProviderId::BuiltIn(ProviderKind::Copilot));

    let effects = reduce(
        &mut session,
        AppAction::DeleteNewApi {
            provider_id: id.clone(),
        },
    );

    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::NewApi(NewApiEffect::DeleteProvider { provider_id, .. }))
            if *provider_id == id
    )));
    assert!(!session.settings_ui.modal.is_confirming_delete_newapi());
    assert!(session.settings_ui.token_editing_provider.is_none());
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
        AppEffect::Common(CommonEffect::NewApi(NewApiEffect::DeleteProvider { provider_id, .. }))
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
    assert!(!session.settings_ui.modal.is_confirming_delete_newapi());
    session.settings_ui.token_editing_provider = Some(ProviderId::BuiltIn(ProviderKind::Copilot));

    let effects = reduce(&mut session, AppAction::ConfirmDeleteNewApi);

    assert!(session.settings_ui.modal.is_confirming_delete_newapi());
    assert!(session.settings_ui.token_editing_provider.is_none());
    assert!(has_render(&effects));
}

#[test]
fn cancel_delete_newapi_clears_confirming_flag() {
    let mut session = make_session();
    session.settings_ui.modal = SettingsModalState::ConfirmingDeleteNewApi;
    session.settings_ui.token_editing_provider = Some(ProviderId::BuiltIn(ProviderKind::Copilot));

    let effects = reduce(&mut session, AppAction::CancelDeleteNewApi);

    assert!(!session.settings_ui.modal.is_confirming_delete_newapi());
    assert!(session.settings_ui.token_editing_provider.is_none());
    assert!(has_render(&effects));
}

#[test]
fn enter_add_newapi_clears_adding_provider() {
    let mut session = make_session();
    session.settings_ui.modal = SettingsModalState::AddingProvider;

    let effects = reduce(&mut session, AppAction::EnterAddNewApi);

    assert!(session.settings_ui.modal.is_newapi_form());
    assert!(!session.settings_ui.modal.is_adding_provider()); // 互斥清除
    assert!(has_render(&effects));
}

#[test]
fn enter_add_newapi_clears_confirming_flags() {
    let mut session = make_session();
    session.settings_ui.modal = SettingsModalState::ConfirmingDeleteScriptProvider;

    let effects = reduce(&mut session, AppAction::EnterAddNewApi);

    assert_eq!(session.settings_ui.modal, SettingsModalState::AddingNewApi);
    assert!(has_render(&effects));
}
