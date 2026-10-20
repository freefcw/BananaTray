use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use crate::application::AppAction;
use crate::models::{AppSettings, ProviderId, SettingsCapability, TokenInputCapability};
use crate::providers::ProviderManager;
use crate::runtime::AppState;

pub(super) fn run(
    state: &Rc<RefCell<AppState>>,
    provider_id: ProviderId,
    request_id: u64,
) -> Vec<AppAction> {
    let state = state.borrow();
    let capability = state
        .session
        .provider_store
        .find_by_id(&provider_id)
        .and_then(|provider| match provider.settings_capability {
            SettingsCapability::TokenInput(capability) => Some(capability),
            _ => None,
        });
    let Some(capability) = capability.filter(|_| !state.token_input_wake_tx.is_closed()) else {
        return vec![AppAction::TokenInputStateResolved {
            provider_id,
            request_id,
            display: None,
        }];
    };
    let manager = state.manager.snapshot();
    let settings = state.session.settings.clone();
    let results = state.token_input_results.clone();
    let wake_tx = state.token_input_wake_tx.clone();
    smol::spawn(async move {
        let action =
            smol::unblock(move || resolve(manager, provider_id, request_id, capability, settings))
                .await;
        if !wake_tx.is_closed() {
            results.push(action);
            let _ = wake_tx.try_send(());
        }
    })
    .detach();
    Vec::new()
}

fn resolve(
    manager: Arc<ProviderManager>,
    provider_id: ProviderId,
    request_id: u64,
    capability: TokenInputCapability,
    settings: AppSettings,
) -> AppAction {
    let display = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        manager.resolve_token_input_state(&provider_id, capability, &settings)
    }))
    .ok();
    if display.is_none() {
        log::warn!(target: "settings", "token display lookup failed for {provider_id}");
    }
    AppAction::TokenInputStateResolved {
        provider_id,
        request_id,
        display,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::{AppEffect, SettingsTab, TokenInputDisplayState};
    use crate::models::{
        ProviderDescriptor, ProviderKind, ProviderMetadata, TokenEditMode, TokenInputState,
    };
    use crate::providers::{
        AiProvider, ProviderCapabilities, ProviderEntry, ProviderManagerHandle,
    };
    use smol::channel::{Receiver, Sender};

    struct LookupProvider {
        id: &'static str,
        marker: &'static str,
        gate: Option<Receiver<()>>,
        started: Sender<&'static str>,
        foreground: std::thread::ThreadId,
    }

    #[async_trait::async_trait]
    impl AiProvider for LookupProvider {
        fn descriptor(&self) -> ProviderDescriptor {
            ProviderDescriptor {
                id: self.id.into(),
                metadata: ProviderMetadata::fallback(ProviderKind::Custom),
            }
        }
    }

    impl ProviderCapabilities for LookupProvider {
        fn settings_capability(&self) -> SettingsCapability {
            crate::providers::copilot_settings_capability()
        }

        fn resolve_token_input_state(&self, settings: &AppSettings) -> Option<TokenInputState> {
            assert_ne!(std::thread::current().id(), self.foreground);
            let local = settings
                .provider
                .credentials
                .get_credential("github_token")
                .is_some();
            self.started.try_send(self.id).unwrap();
            if !local {
                if let Some(gate) = &self.gate {
                    let _ = smol::block_on(gate.recv());
                }
            }
            Some(TokenInputState {
                has_token: true,
                masked: Some(if local { "local" } else { self.marker }.to_string()),
                source_i18n_key: Some(if local {
                    "copilot.source.config_file"
                } else {
                    "copilot.source.env_var"
                }),
                edit_mode: if local {
                    TokenEditMode::EditStored
                } else {
                    TokenEditMode::SetNew
                },
            })
        }
    }

    fn provider(
        id: &'static str,
        marker: &'static str,
        gate: Option<Receiver<()>>,
        started: &Sender<&'static str>,
    ) -> Arc<dyn ProviderEntry> {
        Arc::new(LookupProvider {
            id,
            marker,
            gate,
            started: started.clone(),
            foreground: std::thread::current().id(),
        })
    }

    fn state(providers: Vec<Arc<dyn ProviderEntry>>) -> (Rc<RefCell<AppState>>, Receiver<()>) {
        let (refresh_tx, _refresh_rx) = smol::channel::unbounded();
        let (custom_tx, _custom_rx) = crate::runtime::PersistentJobSender::channel(1);
        let (script_tx, _script_rx) = crate::runtime::BackgroundJobSender::channel(1);
        let (clipboard_tx, _clipboard_rx) = crate::runtime::BackgroundJobSender::channel(1);
        let manager = ProviderManagerHandle::new(ProviderManager::with_custom_providers(providers));
        let mut state = AppState::new(
            crate::refresh::RefreshWorker::detached(refresh_tx),
            custom_tx,
            script_tx,
            clipboard_tx,
            manager,
            AppSettings::default(),
            None,
        );
        state.settings_writer = crate::runtime::SettingsWriter::spawn_for_test(|_| true);
        state.session.settings_ui.selected_provider = id("a:test");
        let wake = state.take_token_input_receiver().unwrap();
        (Rc::new(RefCell::new(state)), wake)
    }

    fn id(value: &str) -> ProviderId {
        ProviderId::Custom(value.to_string())
    }

    fn dispatch(state: &Rc<RefCell<AppState>>, action: AppAction) {
        crate::runtime::dispatch_effects(state, action, |effect| match effect {
            AppEffect::Common(effect) => super::super::run_common_effect(state, effect),
            AppEffect::Context(_) => Vec::new(),
        });
    }

    fn recv<T>(rx: &Receiver<T>) -> T {
        smol::block_on(smol::future::or(
            async { rx.recv().await.expect("lookup channel closed") },
            async {
                smol::Timer::after(std::time::Duration::from_secs(2)).await;
                panic!("token lookup did not complete");
            },
        ))
    }

    fn open(state: &Rc<RefCell<AppState>>) {
        dispatch(state, AppAction::SettingsWindowOpened);
        dispatch(state, AppAction::SetSettingsTab(SettingsTab::Providers));
    }

    fn apply_result(state: &Rc<RefCell<AppState>>, wake: &Receiver<()>) {
        recv(wake);
        let actions = state.borrow().token_input_results.drain();
        for action in actions {
            dispatch(state, action);
        }
    }

    fn display(state: &Rc<RefCell<AppState>>) -> TokenInputDisplayState {
        crate::application::settings_providers_tab_view_state(&state.borrow().session)
            .detail
            .token_input
    }

    fn assert_ready(state: &Rc<RefCell<AppState>>, marker: &str) {
        assert!(
            matches!(display(state), TokenInputDisplayState::Ready(ref value) if value.has_token && value.masked.as_deref() == Some(marker))
        );
    }

    #[test]
    fn external_login_is_loading_until_background_result_arrives() {
        let (started_tx, started_rx) = smol::channel::unbounded();
        let (release_tx, release_rx) = smol::channel::unbounded();
        let (state, wake) = state(vec![provider(
            "a:test",
            "external",
            Some(release_rx),
            &started_tx,
        )]);
        dispatch(&state, AppAction::SettingsWindowOpened);
        assert!(started_rx.try_recv().is_err());
        dispatch(&state, AppAction::SetSettingsTab(SettingsTab::Providers));
        assert_eq!(recv(&started_rx), "a:test");
        assert_eq!(display(&state), TokenInputDisplayState::Loading);
        let request_id = state
            .borrow()
            .session
            .settings_ui
            .token_input_query
            .request_id;
        dispatch(&state, AppAction::RefreshTokenInputState);
        assert_eq!(
            state
                .borrow()
                .session
                .settings_ui
                .token_input_query
                .request_id,
            request_id
        );
        assert!(started_rx.try_recv().is_err());
        release_tx.try_send(()).unwrap();
        apply_result(&state, &wake);
        assert_ready(&state, "external");
    }

    #[test]
    fn switching_provider_accepts_new_result_before_old_lookup_finishes() {
        let (started_tx, started_rx) = smol::channel::unbounded();
        let (release_tx, release_rx) = smol::channel::unbounded();
        let (state, wake) = state(vec![
            provider("a:test", "old-a", Some(release_rx), &started_tx),
            provider("b:test", "current-b", None, &started_tx),
        ]);
        open(&state);
        assert_eq!(recv(&started_rx), "a:test");
        dispatch(&state, AppAction::SelectSettingsProvider(id("b:test")));
        assert_eq!(recv(&started_rx), "b:test");
        apply_result(&state, &wake);
        assert_ready(&state, "current-b");
        release_tx.try_send(()).unwrap();
        apply_result(&state, &wake);
        assert_ready(&state, "current-b");
    }

    #[test]
    fn saved_override_wins_over_pending_external_lookup() {
        let (started_tx, started_rx) = smol::channel::unbounded();
        let (release_tx, release_rx) = smol::channel::unbounded();
        let (state, wake) = state(vec![provider(
            "a:test",
            "external",
            Some(release_rx),
            &started_tx,
        )]);
        open(&state);
        recv(&started_rx);
        dispatch(
            &state,
            AppAction::SaveProviderToken {
                provider_id: id("a:test"),
                token: "test-override".to_string(),
            },
        );
        assert!(
            matches!(display(&state), TokenInputDisplayState::Ready(ref value) if value.has_token && value.edit_mode == TokenEditMode::EditStored)
        );
        recv(&started_rx);
        apply_result(&state, &wake);
        assert_ready(&state, "local");
        release_tx.try_send(()).unwrap();
        apply_result(&state, &wake);
        assert_ready(&state, "local");
    }

    #[test]
    fn registry_reload_rejects_old_registry_lookup() {
        let (started_tx, started_rx) = smol::channel::unbounded();
        let (release_tx, release_rx) = smol::channel::unbounded();
        let (state, wake) = state(vec![provider(
            "a:test",
            "old-registry",
            Some(release_rx),
            &started_tx,
        )]);
        open(&state);
        recv(&started_rx);
        let manager = Arc::new(ProviderManager::with_custom_providers(vec![provider(
            "a:test",
            "new-registry",
            None,
            &started_tx,
        )]));
        let statuses = manager.initial_statuses();
        state.borrow().manager.replace(manager);
        dispatch(
            &state,
            AppAction::RefreshEventReceived(crate::refresh::RefreshEvent::ProvidersReloaded {
                statuses,
            }),
        );
        recv(&started_rx);
        apply_result(&state, &wake);
        assert_ready(&state, "new-registry");
        release_tx.try_send(()).unwrap();
        apply_result(&state, &wake);
        assert_ready(&state, "new-registry");
    }

    #[test]
    fn closing_settings_discards_pending_result_and_stops_queries() {
        let (started_tx, started_rx) = smol::channel::unbounded();
        let (release_tx, release_rx) = smol::channel::unbounded();
        let (state, wake) = state(vec![provider(
            "a:test",
            "external",
            Some(release_rx),
            &started_tx,
        )]);
        open(&state);
        recv(&started_rx);
        dispatch(&state, AppAction::SettingsWindowClosed);
        assert!(matches!(
            state.borrow().session.settings_ui.token_input_query.load,
            crate::application::state::TokenInputLoadState::Idle
        ));
        release_tx.try_send(()).unwrap();
        apply_result(&state, &wake);
        dispatch(&state, AppAction::RefreshTokenInputState);
        assert!(matches!(
            state.borrow().session.settings_ui.token_input_query.load,
            crate::application::state::TokenInputLoadState::Idle
        ));
        assert!(started_rx.try_recv().is_err());
    }

    #[test]
    fn background_recheck_keeps_confirmed_display_until_completion() {
        let (started_tx, started_rx) = smol::channel::unbounded();
        let (release_tx, release_rx) = smol::channel::unbounded();
        let (state, wake) = state(vec![provider(
            "a:test",
            "external",
            Some(release_rx),
            &started_tx,
        )]);
        open(&state);
        recv(&started_rx);
        release_tx.try_send(()).unwrap();
        apply_result(&state, &wake);
        dispatch(&state, AppAction::RefreshTokenInputState);
        recv(&started_rx);
        assert_ready(&state, "external");
        release_tx.try_send(()).unwrap();
        apply_result(&state, &wake);
        assert_ready(&state, "external");
    }
}
