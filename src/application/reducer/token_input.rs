use crate::application::state::{AppSession, SettingsModalState, SettingsTab, TokenInputLoadState};
use crate::application::{AppEffect, ContextEffect, SettingsEffect};
use crate::models::{ProviderId, SettingsCapability, TokenInputState};

pub(super) enum QueryUpdate {
    Sync,
    Invalidate,
    Recheck,
}

pub(super) fn sync_query(
    session: &mut AppSession,
    update: QueryUpdate,
    effects: &mut Vec<AppEffect>,
) {
    let ui = &mut session.settings_ui;
    let provider_id = ui.selected_provider.clone();
    let visible = ui.token_input_query.window_open
        && ui.active_tab == SettingsTab::Providers
        && matches!(
            ui.modal,
            SettingsModalState::Idle
                | SettingsModalState::ConfirmingRemoveProvider
                | SettingsModalState::ConfirmingClearProviderHistory
        )
        && session
            .provider_store
            .find_by_id(&provider_id)
            .is_some_and(|provider| {
                matches!(
                    provider.settings_capability,
                    SettingsCapability::TokenInput(_)
                )
            });
    if !visible {
        if ui.token_input_query.load.provider_id().is_some() {
            ui.token_input_query.load = TokenInputLoadState::Idle;
            effects.push(ContextEffect::Render.into());
        }
        return;
    }
    let same_provider = ui.token_input_query.load.provider_id() == Some(&provider_id);
    if same_provider {
        match update {
            QueryUpdate::Sync => return,
            QueryUpdate::Recheck
                if matches!(
                    ui.token_input_query.load,
                    TokenInputLoadState::Loading { .. }
                ) || ui.token_editing_provider.is_some() =>
            {
                return
            }
            _ => {}
        }
    }
    let previous = match &ui.token_input_query.load {
        TokenInputLoadState::Ready { display, .. }
            if same_provider && matches!(update, QueryUpdate::Recheck) =>
        {
            Some(display.clone())
        }
        _ => None,
    };
    ui.token_input_query.request_id = ui.token_input_query.request_id.wrapping_add(1);
    let request_id = ui.token_input_query.request_id;
    ui.token_input_query.load = TokenInputLoadState::Loading {
        provider_id: provider_id.clone(),
        request_id,
        previous,
    };
    effects.push(
        SettingsEffect::ResolveTokenInputState {
            provider_id,
            request_id,
        }
        .into(),
    );
    effects.push(ContextEffect::Render.into());
}

pub(super) fn window_closed(session: &mut AppSession) {
    session.settings_ui.token_input_query.window_open = false;
}

pub(super) fn apply_resolved(
    session: &mut AppSession,
    provider_id: ProviderId,
    request_id: u64,
    display: Option<TokenInputState>,
    effects: &mut Vec<AppEffect>,
) {
    if !matches!(&session.settings_ui.token_input_query.load,
        TokenInputLoadState::Loading { provider_id: expected, request_id: expected_id, .. }
            if expected == &provider_id && *expected_id == request_id
    ) {
        return;
    }
    session.settings_ui.token_input_query.load = match display {
        Some(display) => TokenInputLoadState::Ready {
            provider_id,
            display,
        },
        None => TokenInputLoadState::Unavailable { provider_id },
    };
    effects.push(ContextEffect::Render.into());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::{
        reduce, settings_providers_tab_view_state, AppAction, CommonEffect, TokenInputDisplayState,
    };
    use crate::models::{AppSettings, ProviderKind, TokenEditMode, TokenInputCapability};

    fn session() -> AppSession {
        let mut provider = crate::models::test_helpers::make_test_provider(
            ProviderKind::Copilot,
            crate::models::ConnectionStatus::Disconnected,
        );
        provider.settings_capability = SettingsCapability::TokenInput(TokenInputCapability {
            credential_key: "github_token",
            placeholder_i18n_key: "copilot.token_placeholder",
            help_tip_i18n_key: "copilot.token_sources_tip",
            title_i18n_key: "copilot.github_login",
            description_i18n_key: "copilot.requires_auth",
            create_url: "https://example.com/token",
        });
        let mut session = AppSession::new(AppSettings::default(), vec![provider]);
        session.settings_ui.selected_provider = ProviderId::BuiltIn(ProviderKind::Copilot);
        session
    }

    fn open(session: &mut AppSession) {
        reduce(session, AppAction::SettingsWindowOpened);
        reduce(session, AppAction::SetSettingsTab(SettingsTab::Providers));
    }

    fn resolved(session: &mut AppSession, has_token: bool) {
        let request_id = session.settings_ui.token_input_query.request_id;
        reduce(
            session,
            AppAction::TokenInputStateResolved {
                provider_id: ProviderId::BuiltIn(ProviderKind::Copilot),
                request_id,
                display: Some(TokenInputState {
                    has_token,
                    masked: None,
                    source_i18n_key: None,
                    edit_mode: TokenEditMode::SetNew,
                }),
            },
        );
    }

    #[test]
    fn only_visible_token_panel_queries_and_reentry_changes_request_id() {
        let mut session = session();
        let effects = reduce(&mut session, AppAction::SettingsWindowOpened);
        assert!(!effects.iter().any(|effect| matches!(
            effect,
            AppEffect::Common(CommonEffect::Settings(
                SettingsEffect::ResolveTokenInputState { .. }
            ))
        )));
        reduce(
            &mut session,
            AppAction::SetSettingsTab(SettingsTab::Providers),
        );
        let old_id = session.settings_ui.token_input_query.request_id;
        assert_eq!(
            settings_providers_tab_view_state(&session)
                .detail
                .token_input,
            TokenInputDisplayState::Loading
        );
        reduce(
            &mut session,
            AppAction::SetSettingsTab(SettingsTab::General),
        );
        assert!(matches!(
            session.settings_ui.token_input_query.load,
            TokenInputLoadState::Idle
        ));
        reduce(&mut session, AppAction::RefreshTokenInputState);
        assert_eq!(session.settings_ui.token_input_query.request_id, old_id);
        reduce(
            &mut session,
            AppAction::SetSettingsTab(SettingsTab::Providers),
        );
        assert!(session.settings_ui.token_input_query.request_id > old_id);
        let effects = reduce(
            &mut session,
            AppAction::TokenInputStateResolved {
                provider_id: ProviderId::BuiltIn(ProviderKind::Copilot),
                request_id: old_id,
                display: None,
            },
        );
        assert!(effects.is_empty());
        assert!(matches!(
            session.settings_ui.token_input_query.load,
            TokenInputLoadState::Loading { .. }
        ));
    }

    #[test]
    fn pending_missing_and_failed_lookup_are_distinct() {
        let mut session = session();
        open(&mut session);
        assert_eq!(
            settings_providers_tab_view_state(&session)
                .detail
                .token_input,
            TokenInputDisplayState::Loading
        );
        let request_id = session.settings_ui.token_input_query.request_id;
        reduce(
            &mut session,
            AppAction::TokenInputStateResolved {
                provider_id: ProviderId::BuiltIn(ProviderKind::Copilot),
                request_id,
                display: None,
            },
        );
        assert_eq!(
            settings_providers_tab_view_state(&session)
                .detail
                .token_input,
            TokenInputDisplayState::Unavailable
        );
        reduce(&mut session, AppAction::RefreshTokenInputState);
        assert_eq!(
            settings_providers_tab_view_state(&session)
                .detail
                .token_input,
            TokenInputDisplayState::Loading
        );
        resolved(&mut session, false);
        assert!(
            matches!(settings_providers_tab_view_state(&session).detail.token_input, TokenInputDisplayState::Ready(value) if !value.has_token)
        );
    }

    #[test]
    fn unavailable_lookup_keeps_local_override_visible() {
        let mut session = session();
        session
            .settings
            .provider
            .credentials
            .set_credential("github_token", "local-token".to_string());
        open(&mut session);
        let request_id = session.settings_ui.token_input_query.request_id;
        reduce(
            &mut session,
            AppAction::TokenInputStateResolved {
                provider_id: ProviderId::BuiltIn(ProviderKind::Copilot),
                request_id,
                display: None,
            },
        );

        assert!(matches!(
            settings_providers_tab_view_state(&session).detail.token_input,
            TokenInputDisplayState::Ready(value)
                if value.has_token && value.edit_mode == TokenEditMode::EditStored
        ));
    }

    #[test]
    fn recheck_preserves_display_and_does_not_interrupt_editor_or_save_context() {
        let mut session = session();
        open(&mut session);
        resolved(&mut session, true);
        reduce(
            &mut session,
            AppAction::SetTokenEditing {
                provider_id: ProviderId::BuiltIn(ProviderKind::Copilot),
                editing: true,
            },
        );
        session.settings_ui.pending_custom_provider_save_request_id = Some(7);
        let request_id = session.settings_ui.token_input_query.request_id;
        reduce(&mut session, AppAction::RefreshTokenInputState);
        assert_eq!(session.settings_ui.token_input_query.request_id, request_id);
        assert_eq!(
            session.settings_ui.pending_custom_provider_save_request_id,
            Some(7)
        );
        reduce(
            &mut session,
            AppAction::SetTokenEditing {
                provider_id: ProviderId::BuiltIn(ProviderKind::Copilot),
                editing: false,
            },
        );
        reduce(&mut session, AppAction::RefreshTokenInputState);
        assert!(matches!(
            session.settings_ui.token_input_query.load,
            TokenInputLoadState::Loading { .. }
        ));
        assert!(
            matches!(settings_providers_tab_view_state(&session).detail.token_input, TokenInputDisplayState::Ready(value) if value.has_token)
        );
    }
}
