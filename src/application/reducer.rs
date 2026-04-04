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
        AppAction::OpenLogDirectory => {
            effects.push(AppEffect::OpenLogDirectory);
        }
        AppAction::CopyToClipboard(text) => {
            effects.push(AppEffect::CopyToClipboard(text));
        }
        AppAction::SelectDebugProvider(kind) => {
            session.settings_ui.debug_selected_provider = Some(kind);
            push_render(&mut effects);
        }
        AppAction::DebugRefreshProvider => {
            if let Some(kind) = session.settings_ui.debug_selected_provider {
                if !session.settings_ui.debug_refresh_active {
                    session.settings_ui.debug_refresh_active = true;
                    // 标记 UI 为 Refreshing
                    session.provider_store.mark_refreshing(kind);
                    effects.push(AppEffect::StartDebugRefresh(kind));
                    push_render(&mut effects);
                }
            }
        }
        AppAction::ClearDebugLogs => {
            effects.push(AppEffect::ClearDebugLogs);
            push_render(&mut effects);
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
        SettingChange::ToggleShowAccountInfo => {
            session.settings.show_account_info = !session.settings.show_account_info;
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
        SettingChange::SetTrayIconStyle(style) => {
            session.settings.tray_icon_style = style;
            effects.push(AppEffect::ApplyTrayIcon(style));
        }
        SettingChange::SetQuotaDisplayMode(mode) => {
            session.settings.quota_display_mode = mode;
        }
        SettingChange::ToggleQuotaVisibility { kind, quota_key } => {
            session.settings.toggle_quota_visibility(kind, quota_key);
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
            // 先记录是否为调试刷新目标（必须在 match 之前，避免被 early return 跳过）
            let is_debug_target = session.settings_ui.debug_refresh_active
                && session.settings_ui.debug_selected_provider == Some(outcome.kind);

            // 处理刷新结果（用 block 限制 return 作用域，确保后续恢复逻辑总能执行）
            'process: {
                if session.provider_store.find(outcome.kind).is_none() {
                    break 'process;
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
                            break 'process;
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
                            break 'process;
                        };
                        provider.mark_unavailable(message);
                        push_render(effects);
                    }
                    RefreshResult::Failed { error, error_kind } => {
                        let Some(provider) = session.provider_store.find_mut(outcome.kind) else {
                            break 'process;
                        };
                        provider.mark_refresh_failed(error, error_kind);
                        push_render(effects);
                    }
                    RefreshResult::SkippedCooldown
                    | RefreshResult::SkippedInFlight
                    | RefreshResult::SkippedDisabled => {}
                }
            }

            // 调试刷新完成后恢复日志级别 — 这段代码无论上面如何分支都一定会执行
            if is_debug_target {
                session.settings_ui.debug_refresh_active = false;
                if let Some(prev_level) = session.settings_ui.debug_prev_log_level.take() {
                    effects.push(AppEffect::RestoreLogLevel(prev_level));
                }
                push_render(effects);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::test_helpers::make_test_provider;
    use crate::models::{AppSettings, ConnectionStatus, ProviderKind};
    use crate::refresh::{RefreshOutcome, RefreshResult};

    fn make_session() -> AppSession {
        let providers = ProviderKind::all()
            .iter()
            .map(|k| make_test_provider(*k, ConnectionStatus::Disconnected))
            .collect();
        AppSession::new(AppSettings::default(), providers)
    }

    /// 构建一个不包含指定 provider 的 session
    fn make_session_without(excluded: ProviderKind) -> AppSession {
        let providers = ProviderKind::all()
            .iter()
            .filter(|k| **k != excluded)
            .map(|k| make_test_provider(*k, ConnectionStatus::Disconnected))
            .collect();
        AppSession::new(AppSettings::default(), providers)
    }

    fn has_effect(effects: &[AppEffect], f: impl Fn(&AppEffect) -> bool) -> bool {
        effects.iter().any(f)
    }

    fn has_render(effects: &[AppEffect]) -> bool {
        has_effect(effects, |e| matches!(e, AppEffect::Render))
    }

    // ── Cadence Dropdown ────────────────────────────────

    #[test]
    fn toggle_cadence_dropdown_flips_state() {
        let mut session = make_session();
        assert!(!session.settings_ui.cadence_dropdown_open);

        let effects = reduce(&mut session, AppAction::ToggleCadenceDropdown);

        assert!(session.settings_ui.cadence_dropdown_open);
        assert!(has_render(&effects));

        reduce(&mut session, AppAction::ToggleCadenceDropdown);
        assert!(!session.settings_ui.cadence_dropdown_open);
    }

    #[test]
    fn update_refresh_cadence_applies_setting_and_closes_dropdown() {
        let mut session = make_session();
        session.settings_ui.cadence_dropdown_open = true;

        let effects = reduce(
            &mut session,
            AppAction::UpdateSetting(SettingChange::RefreshCadence(Some(15))),
        );

        assert_eq!(session.settings.refresh_interval_mins, 15);
        assert!(!session.settings_ui.cadence_dropdown_open);
        assert!(has_effect(&effects, |e| matches!(
            e,
            AppEffect::PersistSettings
        )));
        assert!(has_effect(&effects, |e| matches!(
            e,
            AppEffect::SendRefreshRequest(_)
        )));
        assert!(has_render(&effects));
    }

    // ── ToggleShowAccountInfo ───────────────────────────

    #[test]
    fn toggle_show_account_info_flips_setting() {
        let mut session = make_session();
        assert!(session.settings.show_account_info); // default = true

        let effects = reduce(
            &mut session,
            AppAction::UpdateSetting(SettingChange::ToggleShowAccountInfo),
        );

        assert!(!session.settings.show_account_info);
        assert!(has_effect(&effects, |e| matches!(
            e,
            AppEffect::PersistSettings
        )));
        assert!(has_render(&effects));
    }

    #[test]
    fn toggle_show_account_info_round_trip() {
        let mut session = make_session();

        reduce(
            &mut session,
            AppAction::UpdateSetting(SettingChange::ToggleShowAccountInfo),
        );
        assert!(!session.settings.show_account_info);

        reduce(
            &mut session,
            AppAction::UpdateSetting(SettingChange::ToggleShowAccountInfo),
        );
        assert!(session.settings.show_account_info);
    }

    // ── SelectDebugProvider ─────────────────────────────

    #[test]
    fn select_debug_provider_updates_state() {
        let mut session = make_session();
        assert!(session.settings_ui.debug_selected_provider.is_none());

        let effects = reduce(
            &mut session,
            AppAction::SelectDebugProvider(ProviderKind::Claude),
        );

        assert_eq!(
            session.settings_ui.debug_selected_provider,
            Some(ProviderKind::Claude)
        );
        assert!(has_render(&effects));
    }

    #[test]
    fn select_debug_provider_can_change() {
        let mut session = make_session();
        reduce(
            &mut session,
            AppAction::SelectDebugProvider(ProviderKind::Claude),
        );
        reduce(
            &mut session,
            AppAction::SelectDebugProvider(ProviderKind::Copilot),
        );

        assert_eq!(
            session.settings_ui.debug_selected_provider,
            Some(ProviderKind::Copilot)
        );
    }

    // ── DebugRefreshProvider ────────────────────────────

    #[test]
    fn debug_refresh_without_selection_is_noop() {
        let mut session = make_session();
        let effects = reduce(&mut session, AppAction::DebugRefreshProvider);

        assert!(!session.settings_ui.debug_refresh_active);
        assert!(!has_effect(&effects, |e| matches!(
            e,
            AppEffect::StartDebugRefresh(_)
        )));
    }

    #[test]
    fn debug_refresh_with_selection_produces_effect() {
        let mut session = make_session();
        reduce(
            &mut session,
            AppAction::SelectDebugProvider(ProviderKind::Gemini),
        );

        let effects = reduce(&mut session, AppAction::DebugRefreshProvider);

        assert!(session.settings_ui.debug_refresh_active);
        assert!(has_effect(&effects, |e| matches!(
            e,
            AppEffect::StartDebugRefresh(ProviderKind::Gemini)
        )));
    }

    #[test]
    fn debug_refresh_while_active_is_noop() {
        let mut session = make_session();
        reduce(
            &mut session,
            AppAction::SelectDebugProvider(ProviderKind::Gemini),
        );
        reduce(&mut session, AppAction::DebugRefreshProvider);

        // 再次点击不应重复触发
        let effects = reduce(&mut session, AppAction::DebugRefreshProvider);
        assert!(!has_effect(&effects, |e| matches!(
            e,
            AppEffect::StartDebugRefresh(_)
        )));
    }

    // ── SetTrayIconStyle ───────────────────────────────

    #[test]
    fn set_tray_icon_style_updates_setting_and_produces_effects() {
        use crate::models::TrayIconStyle;

        let mut session = make_session();
        assert_eq!(session.settings.tray_icon_style, TrayIconStyle::Monochrome);

        let effects = reduce(
            &mut session,
            AppAction::UpdateSetting(SettingChange::SetTrayIconStyle(TrayIconStyle::Yellow)),
        );

        assert_eq!(session.settings.tray_icon_style, TrayIconStyle::Yellow);
        assert!(has_effect(&effects, |e| matches!(
            e,
            AppEffect::ApplyTrayIcon(TrayIconStyle::Yellow)
        )));
        assert!(has_effect(&effects, |e| matches!(
            e,
            AppEffect::PersistSettings
        )));
        assert!(has_render(&effects));
    }

    #[test]
    fn set_tray_icon_style_round_trip() {
        use crate::models::TrayIconStyle;

        let mut session = make_session();

        reduce(
            &mut session,
            AppAction::UpdateSetting(SettingChange::SetTrayIconStyle(TrayIconStyle::Colorful)),
        );
        assert_eq!(session.settings.tray_icon_style, TrayIconStyle::Colorful);

        reduce(
            &mut session,
            AppAction::UpdateSetting(SettingChange::SetTrayIconStyle(TrayIconStyle::Monochrome)),
        );
        assert_eq!(session.settings.tray_icon_style, TrayIconStyle::Monochrome);
    }

    // ── SetQuotaDisplayMode ────────────────────────────

    #[test]
    fn set_quota_display_mode_updates_setting_and_produces_effects() {
        use crate::models::QuotaDisplayMode;

        let mut session = make_session();
        assert_eq!(
            session.settings.quota_display_mode,
            QuotaDisplayMode::Remaining
        );

        let effects = reduce(
            &mut session,
            AppAction::UpdateSetting(SettingChange::SetQuotaDisplayMode(QuotaDisplayMode::Used)),
        );

        assert_eq!(session.settings.quota_display_mode, QuotaDisplayMode::Used);
        assert!(has_effect(&effects, |e| matches!(
            e,
            AppEffect::PersistSettings
        )));
        assert!(has_render(&effects));
    }

    #[test]
    fn set_quota_display_mode_round_trip() {
        use crate::models::QuotaDisplayMode;

        let mut session = make_session();

        reduce(
            &mut session,
            AppAction::UpdateSetting(SettingChange::SetQuotaDisplayMode(QuotaDisplayMode::Used)),
        );
        assert_eq!(session.settings.quota_display_mode, QuotaDisplayMode::Used);

        reduce(
            &mut session,
            AppAction::UpdateSetting(SettingChange::SetQuotaDisplayMode(
                QuotaDisplayMode::Remaining,
            )),
        );
        assert_eq!(
            session.settings.quota_display_mode,
            QuotaDisplayMode::Remaining
        );
    }

    // ── ToggleQuotaVisibility ──────────────────────────

    #[test]
    fn toggle_quota_visibility_updates_setting_and_produces_effects() {
        let mut session = make_session();
        assert!(session
            .settings
            .is_quota_visible(ProviderKind::Claude, "session"));

        let effects = reduce(
            &mut session,
            AppAction::UpdateSetting(SettingChange::ToggleQuotaVisibility {
                kind: ProviderKind::Claude,
                quota_key: "session".to_string(),
            }),
        );

        assert!(!session
            .settings
            .is_quota_visible(ProviderKind::Claude, "session"));
        assert!(has_effect(&effects, |e| matches!(
            e,
            AppEffect::PersistSettings
        )));
        assert!(has_render(&effects));
    }

    #[test]
    fn toggle_quota_visibility_round_trip() {
        let mut session = make_session();

        reduce(
            &mut session,
            AppAction::UpdateSetting(SettingChange::ToggleQuotaVisibility {
                kind: ProviderKind::Claude,
                quota_key: "weekly".to_string(),
            }),
        );
        assert!(!session
            .settings
            .is_quota_visible(ProviderKind::Claude, "weekly"));

        reduce(
            &mut session,
            AppAction::UpdateSetting(SettingChange::ToggleQuotaVisibility {
                kind: ProviderKind::Claude,
                quota_key: "weekly".to_string(),
            }),
        );
        assert!(session
            .settings
            .is_quota_visible(ProviderKind::Claude, "weekly"));
    }

    // ── ClearDebugLogs ──────────────────────────────────

    #[test]
    fn clear_debug_logs_produces_effect() {
        let mut session = make_session();
        let effects = reduce(&mut session, AppAction::ClearDebugLogs);

        assert!(has_effect(&effects, |e| matches!(
            e,
            AppEffect::ClearDebugLogs
        )));
        assert!(has_render(&effects));
    }

    // ── RefreshEvent::Finished + debug restore ──────────

    #[test]
    fn finished_event_restores_debug_state() {
        let mut session = make_session();
        let kind = ProviderKind::Claude;

        // 模拟调试刷新状态
        session.settings_ui.debug_selected_provider = Some(kind);
        session.settings_ui.debug_refresh_active = true;
        session.settings_ui.debug_prev_log_level = Some(log::LevelFilter::Info);

        let outcome = RefreshOutcome {
            kind,
            result: RefreshResult::Failed {
                error: "test error".to_string(),
                error_kind: crate::models::ErrorKind::NetworkError,
            },
        };
        let mut effects = vec![];
        apply_refresh_event(&mut session, RefreshEvent::Finished(outcome), &mut effects);

        // 即使刷新失败，也应该恢复
        assert!(!session.settings_ui.debug_refresh_active);
        assert!(session.settings_ui.debug_prev_log_level.is_none());
        assert!(has_effect(&effects, |e| matches!(
            e,
            AppEffect::RestoreLogLevel(log::LevelFilter::Info)
        )));
    }

    #[test]
    fn finished_event_for_other_provider_does_not_restore() {
        let mut session = make_session();

        // 调试刷新 Claude，但完成的是 Gemini
        session.settings_ui.debug_selected_provider = Some(ProviderKind::Claude);
        session.settings_ui.debug_refresh_active = true;
        session.settings_ui.debug_prev_log_level = Some(log::LevelFilter::Info);

        let outcome = RefreshOutcome {
            kind: ProviderKind::Gemini,
            result: RefreshResult::SkippedCooldown,
        };
        let mut effects = vec![];
        apply_refresh_event(&mut session, RefreshEvent::Finished(outcome), &mut effects);

        // 不应恢复
        assert!(session.settings_ui.debug_refresh_active);
        assert!(session.settings_ui.debug_prev_log_level.is_some());
        assert!(!has_effect(&effects, |e| matches!(
            e,
            AppEffect::RestoreLogLevel(_)
        )));
    }

    #[test]
    fn finished_restore_survives_unknown_provider() {
        // 构建一个不包含 Claude 的 session
        let mut session = make_session_without(ProviderKind::Claude);
        let kind = ProviderKind::Claude;

        // 调试刷新中，但 provider_store 中该 provider 不存在
        session.settings_ui.debug_selected_provider = Some(kind);
        session.settings_ui.debug_refresh_active = true;
        session.settings_ui.debug_prev_log_level = Some(log::LevelFilter::Warn);

        let outcome = RefreshOutcome {
            kind,
            result: RefreshResult::Failed {
                error: "gone".to_string(),
                error_kind: crate::models::ErrorKind::Unknown,
            },
        };
        let mut effects = vec![];
        apply_refresh_event(&mut session, RefreshEvent::Finished(outcome), &mut effects);

        // 关键：即使 provider 不在 store 中，恢复逻辑仍必须执行
        assert!(!session.settings_ui.debug_refresh_active);
        assert!(has_effect(&effects, |e| matches!(
            e,
            AppEffect::RestoreLogLevel(log::LevelFilter::Warn)
        )));
    }
}
