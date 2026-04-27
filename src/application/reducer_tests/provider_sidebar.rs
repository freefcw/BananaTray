use super::common::{has_effect, has_render, make_custom_token_provider, make_session, pid};
use crate::application::{
    reduce, AppAction, AppEffect, CommonEffect, RefreshEffect, SettingsEffect,
};
use crate::models::{ProviderId, ProviderKind};
use crate::refresh::RefreshRequest;

// ── MoveProviderToIndex（拖拽排序）──────────────────

#[test]
fn move_provider_to_index_persists_and_renders() {
    let mut session = make_session();
    // Claude 默认在 index 0，移动到末尾以确保触发状态变更
    let total = ProviderKind::all().len();
    let effects = reduce(
        &mut session,
        AppAction::MoveProviderToIndex {
            id: pid(ProviderKind::Claude),
            target_index: total - 1,
        },
    );

    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::PersistSettings))
    )));
    assert!(has_render(&effects));
}

#[test]
fn move_provider_to_same_index_produces_no_effects() {
    let mut session = make_session();
    // 首先获取 claude 的当前位置
    let custom_ids = session.provider_store.custom_provider_ids();
    let ordered = session.settings.provider.ordered_provider_ids(&custom_ids);
    let claude_index = ordered
        .iter()
        .position(|id| *id == pid(ProviderKind::Claude))
        .unwrap();

    let effects = reduce(
        &mut session,
        AppAction::MoveProviderToIndex {
            id: pid(ProviderKind::Claude),
            target_index: claude_index,
        },
    );

    assert!(effects.is_empty());
}

// ── Sidebar dynamic list ────────────────────────────

#[test]
fn enter_add_provider_sets_flag_and_clears_newapi() {
    let mut session = make_session();
    session.settings_ui.adding_newapi = true;

    let effects = reduce(&mut session, AppAction::EnterAddProvider);

    assert!(session.settings_ui.adding_provider);
    assert!(!session.settings_ui.adding_newapi); // 互斥
    assert!(has_render(&effects));
}

#[test]
fn cancel_add_provider_clears_flag() {
    let mut session = make_session();
    session.settings_ui.adding_provider = true;

    let effects = reduce(&mut session, AppAction::CancelAddProvider);

    assert!(!session.settings_ui.adding_provider);
    assert!(has_render(&effects));
}

#[test]
fn add_provider_to_sidebar_persists_and_selects() {
    let mut session = make_session();
    // 预设 sidebar 只有 claude
    session.settings.provider.sidebar_providers = vec!["claude".into()];
    session.settings_ui.adding_provider = true;

    let id = pid(ProviderKind::Gemini);
    let effects = reduce(&mut session, AppAction::AddProviderToSidebar(id.clone()));

    // sidebar 现在包含 gemini
    assert!(session
        .settings
        .provider
        .sidebar_providers
        .contains(&"gemini".to_string()));
    // 选中了刚添加的 provider
    assert_eq!(session.settings_ui.selected_provider, id);
    // 退出添加模式
    assert!(!session.settings_ui.adding_provider);
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::PersistSettings))
    )));
    assert!(has_render(&effects));
}

#[test]
fn remove_provider_from_sidebar_disables_and_persists() {
    let mut session = make_session();
    session.settings.provider.sidebar_providers = vec!["claude".into(), "gemini".into()];
    session
        .settings
        .provider
        .set_enabled(&pid(ProviderKind::Claude), true);
    session
        .settings
        .provider
        .set_enabled(&pid(ProviderKind::Gemini), true);

    let effects = reduce(
        &mut session,
        AppAction::RemoveProviderFromSidebar(pid(ProviderKind::Claude)),
    );

    // claude 不在 sidebar 中了
    assert!(!session
        .settings
        .provider
        .sidebar_providers
        .contains(&"claude".to_string()));
    // claude 被 disable
    assert!(!session
        .settings
        .provider
        .is_enabled(&pid(ProviderKind::Claude)));
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::PersistSettings))
    )));
    assert!(has_render(&effects));
}

#[test]
fn remove_last_sidebar_provider_enters_add_mode() {
    let mut session = make_session();
    session.settings.provider.sidebar_providers = vec!["claude".into()];
    session
        .settings
        .provider
        .set_enabled(&pid(ProviderKind::Claude), true);

    let effects = reduce(
        &mut session,
        AppAction::RemoveProviderFromSidebar(pid(ProviderKind::Claude)),
    );

    // sidebar 已空
    assert!(session.settings.provider.sidebar_providers.is_empty());
    // 自动进入添加模式
    assert!(session.settings_ui.adding_provider);
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::PersistSettings))
    )));
    assert!(has_render(&effects));
}

#[test]
fn remove_nonexistent_provider_from_sidebar_is_noop() {
    let mut session = make_session();
    session.settings.provider.sidebar_providers = vec!["claude".into()];

    let effects = reduce(
        &mut session,
        AppAction::RemoveProviderFromSidebar(pid(ProviderKind::Gemini)),
    );

    // sidebar 不变
    assert_eq!(session.settings.provider.sidebar_providers.len(), 1);
    // 无持久化
    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::PersistSettings))
    )));
    // 仍有 render（Render effect 在 if 外无条件 push）
    assert!(has_render(&effects));
}

// ── 二次确认状态 ──────────────────────────────────────

#[test]
fn confirm_remove_provider_sets_confirming_flag() {
    let mut session = make_session();
    assert!(!session.settings_ui.confirming_remove_provider);

    let effects = reduce(&mut session, AppAction::ConfirmRemoveProvider);

    assert!(session.settings_ui.confirming_remove_provider);
    assert!(has_render(&effects));
}

#[test]
fn cancel_remove_provider_clears_confirming_flag() {
    let mut session = make_session();
    session.settings_ui.confirming_remove_provider = true;

    let effects = reduce(&mut session, AppAction::CancelRemoveProvider);

    assert!(!session.settings_ui.confirming_remove_provider);
    assert!(has_render(&effects));
}

#[test]
fn remove_provider_resets_confirming_flag() {
    let mut session = make_session();
    session.settings.provider.sidebar_providers = vec!["claude".into(), "gemini".into()];
    session.settings_ui.confirming_remove_provider = true;

    reduce(
        &mut session,
        AppAction::RemoveProviderFromSidebar(pid(ProviderKind::Claude)),
    );

    assert!(!session.settings_ui.confirming_remove_provider);
}

#[test]
fn select_provider_resets_confirming_flags() {
    let mut session = make_session();
    session.settings_ui.confirming_remove_provider = true;
    session.settings_ui.confirming_delete_newapi = true;
    session.settings_ui.token_editing_provider = Some(pid(ProviderKind::Copilot));

    reduce(
        &mut session,
        AppAction::SelectSettingsProvider(pid(ProviderKind::Gemini)),
    );

    assert!(!session.settings_ui.confirming_remove_provider);
    assert!(!session.settings_ui.confirming_delete_newapi);
    assert!(session.settings_ui.token_editing_provider.is_none());
}

// ── Token Editing / Saving ────────────────────────────

#[test]
fn set_token_editing_enables_editing() {
    let mut session = make_session();
    assert!(session.settings_ui.token_editing_provider.is_none());

    let effects = reduce(
        &mut session,
        AppAction::SetTokenEditing {
            provider_id: pid(ProviderKind::Copilot),
            editing: true,
        },
    );

    assert_eq!(
        session.settings_ui.token_editing_provider,
        Some(pid(ProviderKind::Copilot))
    );
    assert!(has_render(&effects));
}

#[test]
fn set_token_editing_disables_editing() {
    let mut session = make_session();
    session.settings_ui.token_editing_provider = Some(pid(ProviderKind::Copilot));

    let effects = reduce(
        &mut session,
        AppAction::SetTokenEditing {
            provider_id: pid(ProviderKind::Copilot),
            editing: false,
        },
    );

    assert!(session.settings_ui.token_editing_provider.is_none());
    assert!(has_render(&effects));
}

#[test]
fn save_provider_token_stores_credential_and_persists() {
    let mut session = make_session();
    session.settings_ui.token_editing_provider = Some(pid(ProviderKind::Copilot));

    let effects = reduce(
        &mut session,
        AppAction::SaveProviderToken {
            provider_id: pid(ProviderKind::Copilot),
            token: "ghp_test123".to_string(),
        },
    );

    // token 已存储
    assert_eq!(
        session
            .settings
            .provider
            .credentials
            .get_credential("github_token"),
        Some("ghp_test123")
    );
    // 编辑状态已关闭
    assert!(session.settings_ui.token_editing_provider.is_none());
    // 产出 PersistSettings
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::PersistSettings))
    )));
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Refresh(RefreshEffect::SendRequest(
            RefreshRequest::UpdateConfig {
                provider_credentials,
                ..
            }
        ))) if provider_credentials.get_credential("github_token") == Some("ghp_test123")
    )));
    assert!(has_render(&effects));
}

#[test]
fn save_provider_token_empty_does_not_persist() {
    let mut session = make_session();
    session.settings_ui.token_editing_provider = Some(pid(ProviderKind::Copilot));

    let effects = reduce(
        &mut session,
        AppAction::SaveProviderToken {
            provider_id: pid(ProviderKind::Copilot),
            token: "   ".to_string(), // 空白
        },
    );

    // 不应存储
    assert!(session
        .settings
        .provider
        .credentials
        .get_credential("github_token")
        .is_none());
    // 编辑状态仍关闭
    assert!(session.settings_ui.token_editing_provider.is_none());
    // 不应产出 PersistSettings
    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::PersistSettings))
    )));
}

#[test]
fn save_provider_token_without_capability_does_not_persist() {
    let mut session = make_session();
    // Claude 没有 TokenInput capability
    session.settings_ui.token_editing_provider = Some(pid(ProviderKind::Claude));

    let effects = reduce(
        &mut session,
        AppAction::SaveProviderToken {
            provider_id: pid(ProviderKind::Claude),
            token: "some_token".to_string(),
        },
    );

    // 不应产出 PersistSettings（capability 不匹配）
    assert!(!has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::PersistSettings))
    )));
    // 编辑状态仍关闭
    assert!(session.settings_ui.token_editing_provider.is_none());
}

#[test]
fn save_provider_token_supports_arbitrary_credential_key() {
    let custom_id = ProviderId::Custom("custom-token:api".to_string());
    let mut session = make_session();
    session
        .provider_store
        .providers
        .push(make_custom_token_provider(
            "custom-token:api",
            "custom_token",
        ));
    session.settings_ui.token_editing_provider = Some(custom_id.clone());

    let effects = reduce(
        &mut session,
        AppAction::SaveProviderToken {
            provider_id: custom_id,
            token: "custom-secret".to_string(),
        },
    );

    assert_eq!(
        session
            .settings
            .provider
            .credentials
            .get_credential("custom_token"),
        Some("custom-secret")
    );
    assert!(has_effect(&effects, |e| matches!(
        e,
        AppEffect::Common(CommonEffect::Settings(SettingsEffect::PersistSettings))
    )));
}
