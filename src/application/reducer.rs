use crate::app_state::{AppSession, SettingsTab};
use crate::application::{AppAction, AppEffect, ProviderOrderDirection, SettingChange};
use crate::models::{NavTab, ProviderKind};
use crate::refresh::{RefreshEvent, RefreshReason, RefreshRequest, RefreshResult};
use log::{debug, info};

pub fn reduce(session: &mut AppSession, action: AppAction) -> Vec<AppEffect> {
    let mut effects = Vec::new();

    match action {
        AppAction::SelectNavTab(tab) => {
            session.nav.switch_to(tab);
            push_render(&mut effects);
        }
        AppAction::SetSettingsTab(tab) => {
            session.settings_ui.active_tab = tab;
            push_render(&mut effects);
        }
        AppAction::SelectSettingsProvider(kind) => {
            session.settings_ui.selected_provider = kind;
            push_render(&mut effects);
        }
        AppAction::ToggleCadenceDropdown => {
            session.settings_ui.cadence_dropdown_open = !session.settings_ui.cadence_dropdown_open;
            push_render(&mut effects);
        }
        AppAction::SetCopilotTokenEditing(editing) => {
            session.settings_ui.copilot_token_editing = editing;
            push_render(&mut effects);
        }
        AppAction::SaveCopilotToken(token) => {
            let token = token.trim().to_string();
            if !token.is_empty() {
                session.settings.providers.github_token = Some(token);
                effects.push(AppEffect::PersistSettings);
            }
            session.settings_ui.copilot_token_editing = false;
            push_render(&mut effects);
        }
        AppAction::ReorderProvider { kind, direction } => {
            let moved = match direction {
                ProviderOrderDirection::Up => session.settings.move_provider_up(kind),
                ProviderOrderDirection::Down => session.settings.move_provider_down(kind),
            };
            if moved {
                effects.push(AppEffect::PersistSettings);
                push_render(&mut effects);
            }
        }
        AppAction::UpdateSetting(change) => {
            apply_setting_change(session, change, &mut effects);
        }
        AppAction::RefreshProvider { kind, reason } => {
            request_provider_refresh(session, kind, reason, &mut effects);
        }
        AppAction::ToggleProvider(kind) => {
            toggle_provider(session, kind, &mut effects);
        }
        AppAction::RefreshEventReceived(event) => {
            apply_refresh_event(session, event, &mut effects);
        }
        AppAction::OpenSettings { provider } => {
            if let Some(kind) = provider {
                session.settings_ui.selected_provider = kind;
                session.settings_ui.active_tab = SettingsTab::Providers;
            }
            effects.push(AppEffect::OpenSettingsWindow);
        }
        AppAction::OpenDashboard(kind) => {
            if let Some(provider) = session.provider_store.find(kind) {
                let url = provider.dashboard_url().trim();
                if !url.is_empty() {
                    effects.push(AppEffect::OpenUrl(url.to_string()));
                }
            }
        }
        AppAction::OpenUrl(url) => effects.push(AppEffect::OpenUrl(url)),
        AppAction::UpdateLogLevel(level) => {
            effects.push(AppEffect::UpdateLogLevel(level));
            push_render(&mut effects);
        }
        AppAction::SendDebugNotification(kind) => {
            effects.push(AppEffect::SendDebugNotification {
                kind,
                with_sound: session.settings.notification_sound,
            });
        }
        AppAction::QuitApp => effects.push(AppEffect::QuitApp),
    }

    effects
}

fn apply_setting_change(
    session: &mut AppSession,
    change: SettingChange,
    effects: &mut Vec<AppEffect>,
) {
    match change {
        SettingChange::ToggleAutoHideWindow => {
            session.settings.auto_hide_window = !session.settings.auto_hide_window;
        }
        SettingChange::ToggleStartAtLogin => {
            let new_val = !session.settings.start_at_login;
            session.settings.start_at_login = new_val;
            effects.push(AppEffect::SyncAutoLaunch(new_val));
        }
        SettingChange::ToggleSessionQuotaNotifications => {
            session.settings.session_quota_notifications =
                !session.settings.session_quota_notifications;
        }
        SettingChange::ToggleNotificationSound => {
            session.settings.notification_sound = !session.settings.notification_sound;
        }
        SettingChange::ToggleShowDashboardButton => {
            session.settings.show_dashboard_button = !session.settings.show_dashboard_button;
        }
        SettingChange::ToggleShowRefreshButton => {
            session.settings.show_refresh_button = !session.settings.show_refresh_button;
        }
        SettingChange::ToggleShowDebugTab => {
            let new_val = !session.settings.show_debug_tab;
            session.settings.show_debug_tab = new_val;
            if !new_val && session.settings_ui.active_tab == SettingsTab::Debug {
                session.settings_ui.active_tab = SettingsTab::General;
            }
        }
        SettingChange::Theme(theme) => {
            session.settings.theme = theme;
        }
        SettingChange::Language(language) => {
            session.settings.language = language.clone();
            effects.push(AppEffect::ApplyLocale(language));
        }
        SettingChange::RefreshCadence(mins) => {
            session.settings.refresh_interval_mins = mins.unwrap_or(0);
            session.settings_ui.cadence_dropdown_open = false;
            effects.push(AppEffect::SendRefreshRequest(build_config_sync_request(
                session,
            )));
        }
    }

    effects.push(AppEffect::PersistSettings);
    push_render(effects);
}

fn request_provider_refresh(
    session: &mut AppSession,
    kind: ProviderKind,
    reason: RefreshReason,
    effects: &mut Vec<AppEffect>,
) {
    if !session.settings.is_provider_enabled(kind) {
        debug!(
            target: "refresh",
            "ignoring refresh request for disabled provider {:?}",
            kind
        );
        return;
    }

    session.provider_store.mark_refreshing(kind);
    effects.push(AppEffect::SendRefreshRequest(RefreshRequest::RefreshOne {
        kind,
        reason,
    }));
    push_render(effects);
}

fn toggle_provider(session: &mut AppSession, kind: ProviderKind, effects: &mut Vec<AppEffect>) {
    let new_val = !session.settings.is_provider_enabled(kind);
    info!(
        target: "providers",
        "toggling provider {:?} from {} to {}",
        kind,
        !new_val,
        new_val
    );
    session.settings.set_provider_enabled(kind, new_val);

    if let Some(provider) = session.provider_store.find_mut(kind) {
        provider.enabled = new_val;
    }

    if new_val {
        session.nav.switch_to(NavTab::Provider(kind));
    } else {
        session.nav.fallback_on_disable(kind, &session.settings);
    }

    effects.push(AppEffect::PersistSettings);
    effects.push(AppEffect::SendRefreshRequest(build_config_sync_request(
        session,
    )));
    if new_val {
        request_provider_refresh(session, kind, RefreshReason::ProviderToggled, effects);
    } else {
        push_render(effects);
    }
}

fn apply_refresh_event(
    session: &mut AppSession,
    event: RefreshEvent,
    effects: &mut Vec<AppEffect>,
) {
    match event {
        RefreshEvent::Started { kind } => {
            session.provider_store.mark_refreshing(kind);
            push_render(effects);
        }
        RefreshEvent::Finished(outcome) => {
            if session.provider_store.find(outcome.kind).is_none() {
                return;
            }

            match outcome.result {
                RefreshResult::Success { data } => {
                    info!(
                        target: "providers",
                        "provider {:?} refresh succeeded: {} quotas",
                        outcome.kind,
                        data.quotas.len()
                    );
                    let provider_name = session
                        .provider_store
                        .find(outcome.kind)
                        .map(|provider| provider.display_name().to_string())
                        .unwrap_or_else(|| format!("{:?}", outcome.kind));
                    if let Some(alert) =
                        session
                            .alert_tracker
                            .update(outcome.kind, &provider_name, &data.quotas)
                    {
                        if session.settings.session_quota_notifications {
                            effects.push(AppEffect::SendQuotaNotification {
                                alert,
                                with_sound: session.settings.notification_sound,
                            });
                        }
                    }
                    let Some(provider) = session.provider_store.find_mut(outcome.kind) else {
                        return;
                    };
                    provider.mark_refresh_succeeded(data);
                    push_render(effects);
                }
                RefreshResult::Unavailable { message } => {
                    debug!(
                        target: "providers",
                        "provider {:?} unavailable: {}",
                        outcome.kind,
                        message
                    );
                    let Some(provider) = session.provider_store.find_mut(outcome.kind) else {
                        return;
                    };
                    provider.mark_unavailable(message);
                    push_render(effects);
                }
                RefreshResult::Failed { error, error_kind } => {
                    let Some(provider) = session.provider_store.find_mut(outcome.kind) else {
                        return;
                    };
                    provider.mark_refresh_failed(error, error_kind);
                    push_render(effects);
                }
                RefreshResult::SkippedCooldown
                | RefreshResult::SkippedInFlight
                | RefreshResult::SkippedDisabled => {}
            }
        }
    }
}

pub fn build_config_sync_request(session: &AppSession) -> RefreshRequest {
    let enabled: Vec<ProviderKind> = ProviderKind::all()
        .iter()
        .filter(|kind| session.settings.is_provider_enabled(**kind))
        .copied()
        .collect();

    RefreshRequest::UpdateConfig {
        interval_mins: session.settings.refresh_interval_mins,
        enabled,
    }
}

fn push_render(effects: &mut Vec<AppEffect>) {
    effects.push(AppEffect::Render);
}
