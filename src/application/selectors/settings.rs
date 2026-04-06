//! Settings 窗口的 selector 函数
//!
//! 将 AppSession → Settings ViewModel 的转换逻辑集中于此。

use super::format::format_last_updated;
use super::*;
use crate::app_state::AppSession;
use crate::models::{ConnectionStatus, ProviderId, ProviderKind, ProviderStatus};
use rust_i18n::t;

pub fn settings_providers_tab_view_state(session: &AppSession) -> SettingsProvidersTabViewState {
    let custom_ids = session.provider_store.custom_provider_ids();
    let ordered = session.settings.ordered_provider_ids(&custom_ids);
    let selected = &session.settings_ui.selected_provider;

    let items = ordered
        .iter()
        .enumerate()
        .map(|(index, id)| {
            let provider = session.provider_store.find_by_id(id);
            SettingsProviderListItemViewState {
                id: id.clone(),
                icon: provider
                    .map(|provider| provider.icon_asset().to_string())
                    .unwrap_or_else(|| "src/icons/provider-unknown.svg".to_string()),
                display_name: provider
                    .map(|provider| provider.display_name().to_string())
                    .unwrap_or_else(|| format!("{}", id)),
                is_selected: id == selected,
                is_enabled: session.settings.is_enabled(id),
                can_move_up: index > 0,
                can_move_down: index + 1 < ordered.len(),
            }
        })
        .collect();

    SettingsProvidersTabViewState {
        items,
        detail: settings_provider_detail_view_state(session, selected),
    }
}

// ── 内部 Helper ─────────────────────────────────────────────

fn settings_provider_detail_view_state(
    session: &AppSession,
    id: &ProviderId,
) -> SettingsProviderDetailViewState {
    let provider = session.provider_store.find_by_id(id);
    let is_enabled = session.settings.is_enabled(id);

    let (icon, display_name, subtitle) = if let Some(provider) = provider {
        (
            provider.icon_asset().to_string(),
            provider.display_name().to_string(),
            settings_provider_subtitle(provider),
        )
    } else {
        (
            "src/icons/provider-unknown.svg".to_string(),
            format!("{}", id),
            format!("{} · {}", id, t!("provider.not_available")),
        )
    };

    let quota_visibility = provider
        .map(|p| {
            p.quotas
                .iter()
                .map(|q| {
                    let quota_key = q.quota_type.stable_key();
                    QuotaVisibilityItem {
                        label: q.label.clone(),
                        quota_key: quota_key.clone(),
                        visible: session.settings.is_quota_visible(id.kind(), &quota_key),
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    SettingsProviderDetailViewState {
        id: id.clone(),
        icon,
        display_name,
        subtitle,
        is_enabled,
        info: settings_provider_info_view_state(provider, is_enabled),
        usage: settings_provider_usage_view_state(provider, is_enabled),
        settings_mode: match id {
            ProviderId::BuiltIn(ProviderKind::Copilot) => ProviderSettingsMode::Interactive,
            _ => ProviderSettingsMode::AutoManaged,
        },
        quota_display_mode: session.settings.quota_display_mode,
        quota_visibility,
    }
}

fn settings_provider_info_view_state(
    provider: Option<&ProviderStatus>,
    is_enabled: bool,
) -> SettingsProviderInfoViewState {
    let state_text = if is_enabled {
        t!("provider.state.enabled").to_string()
    } else {
        t!("provider.state.disabled").to_string()
    };
    let source_text = t!("provider.source.auto").to_string();
    let updated_text = provider
        .map(format_last_updated)
        .unwrap_or_else(|| t!("provider.not_fetched").to_string());

    let (status_text, status_kind) = provider
        .map(|provider| match provider.connection {
            ConnectionStatus::Connected => (
                t!("provider.status.operational").to_string(),
                SettingsProviderStatusKind::Success,
            ),
            ConnectionStatus::Disconnected => (
                t!("provider.status.not_detected").to_string(),
                SettingsProviderStatusKind::Neutral,
            ),
            ConnectionStatus::Refreshing => (
                t!("provider.status.refreshing").to_string(),
                SettingsProviderStatusKind::Neutral,
            ),
            ConnectionStatus::Error => (
                t!("provider.status.error").to_string(),
                SettingsProviderStatusKind::Error,
            ),
        })
        .unwrap_or_else(|| {
            (
                t!("provider.status.unknown").to_string(),
                SettingsProviderStatusKind::Neutral,
            )
        });

    SettingsProviderInfoViewState {
        state_text,
        source_text,
        updated_text,
        status_text,
        status_kind,
    }
}

fn settings_provider_usage_view_state(
    provider: Option<&ProviderStatus>,
    is_enabled: bool,
) -> SettingsProviderUsageViewState {
    if !is_enabled {
        return SettingsProviderUsageViewState::Disabled {
            message: t!("provider.enable_tracking").to_string(),
        };
    }

    let Some(provider) = provider else {
        return SettingsProviderUsageViewState::Missing {
            message: t!("provider.not_available").to_string(),
        };
    };

    if !provider.quotas.is_empty() {
        return SettingsProviderUsageViewState::Quotas {
            quotas: provider.quotas.clone(),
        };
    }

    if provider.connection == ConnectionStatus::Error {
        return SettingsProviderUsageViewState::Error {
            title: t!("provider.last_fetch_failed", name = provider.display_name()).to_string(),
            message: provider
                .error_message
                .clone()
                .unwrap_or_else(|| t!("provider.unknown_error").to_string()),
        };
    }

    SettingsProviderUsageViewState::Empty {
        message: t!("provider.no_usage").to_string(),
    }
}

fn settings_provider_subtitle(provider: &ProviderStatus) -> String {
    let source = provider.source_label();
    match provider.connection {
        ConnectionStatus::Error => t!("provider.detail.last_failed", source = source).to_string(),
        ConnectionStatus::Refreshing => {
            t!("provider.detail.refreshing", source = source).to_string()
        }
        ConnectionStatus::Connected => {
            if provider.last_refreshed_instant.is_some() {
                let time = format_last_updated(provider).to_lowercase();
                t!("provider.detail.updated", source = source, time = time).to_string()
            } else {
                t!("provider.detail.not_fetched", source = source).to_string()
            }
        }
        ConnectionStatus::Disconnected => {
            t!("provider.detail.not_detected", source = source).to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::test_helpers::{
        make_test_provider as make_provider, setup_test_locale as setup_locale,
    };
    use crate::models::{AppSettings, ConnectionStatus, ProviderKind, ProviderStatus};

    fn pid(kind: ProviderKind) -> ProviderId {
        ProviderId::BuiltIn(kind)
    }

    fn make_session(
        settings: AppSettings,
        selected_provider: ProviderId,
        providers: Vec<ProviderStatus>,
    ) -> AppSession {
        let mut session = AppSession::new(settings, providers);
        session.settings_ui.selected_provider = selected_provider;
        session
    }

    #[test]
    fn settings_providers_tab_marks_reorder_boundaries() {
        let _locale_guard = setup_locale();
        let mut settings = AppSettings {
            provider_order: vec!["gemini".into(), "claude".into(), "copilot".into()],
            ..Default::default()
        };
        settings.set_provider_enabled(ProviderKind::Gemini, true);
        settings.set_provider_enabled(ProviderKind::Claude, true);
        settings.set_provider_enabled(ProviderKind::Copilot, true);

        let session = make_session(
            settings,
            pid(ProviderKind::Claude),
            vec![
                make_provider(ProviderKind::Gemini, ConnectionStatus::Connected),
                make_provider(ProviderKind::Claude, ConnectionStatus::Connected),
                make_provider(ProviderKind::Copilot, ConnectionStatus::Connected),
            ],
        );

        let view_state = settings_providers_tab_view_state(&session);

        assert_eq!(view_state.items[0].id, pid(ProviderKind::Gemini));
        assert!(!view_state.items[0].can_move_up);
        assert!(view_state.items[0].can_move_down);
        assert_eq!(view_state.items[1].id, pid(ProviderKind::Claude));
        assert!(view_state.items[1].is_selected);
        assert!(view_state.items[1].can_move_up);
        assert!(view_state.items[1].can_move_down);
    }

    #[test]
    fn settings_provider_detail_reports_disabled_usage() {
        let _locale_guard = setup_locale();
        let mut settings = AppSettings::default();
        settings.set_provider_enabled(ProviderKind::Claude, false);

        let session = make_session(
            settings,
            pid(ProviderKind::Claude),
            vec![make_provider(
                ProviderKind::Claude,
                ConnectionStatus::Disconnected,
            )],
        );

        let view_state = settings_providers_tab_view_state(&session);

        assert!(!view_state.detail.is_enabled);
        assert_eq!(view_state.detail.info.state_text, "Disabled");
        assert!(matches!(
            view_state.detail.usage,
            SettingsProviderUsageViewState::Disabled { .. }
        ));
    }

    #[test]
    fn settings_provider_detail_reports_error_usage() {
        let _locale_guard = setup_locale();
        let mut settings = AppSettings::default();
        settings.set_provider_enabled(ProviderKind::Copilot, true);

        let mut provider = make_provider(ProviderKind::Copilot, ConnectionStatus::Error);
        provider.error_message = Some("boom".to_string());

        let session = make_session(settings, pid(ProviderKind::Copilot), vec![provider]);
        let view_state = settings_providers_tab_view_state(&session);

        assert_eq!(
            view_state.detail.settings_mode,
            ProviderSettingsMode::Interactive
        );
        assert_eq!(
            view_state.detail.info.status_kind,
            SettingsProviderStatusKind::Error
        );
        assert!(matches!(
            view_state.detail.usage,
            SettingsProviderUsageViewState::Error { .. }
        ));
    }

    // ── quota_visibility 构建 ────────────────────────────

    #[test]
    fn settings_detail_builds_quota_visibility_from_stable_key() {
        use crate::models::{QuotaInfo, QuotaType};

        let _locale_guard = setup_locale();
        let mut settings = AppSettings::default();
        settings.set_provider_enabled(ProviderKind::Claude, true);
        // 隐藏 session 类型配额
        settings.toggle_quota_visibility(ProviderKind::Claude, "session".to_string());

        let mut provider = make_provider(ProviderKind::Claude, ConnectionStatus::Connected);
        provider.quotas = vec![
            QuotaInfo::with_details(
                String::from("Session (5h)"),
                30.0,
                100.0,
                QuotaType::Session,
                None,
            ),
            QuotaInfo::with_details(String::from("Weekly"), 50.0, 100.0, QuotaType::Weekly, None),
        ];

        let session = make_session(settings, pid(ProviderKind::Claude), vec![provider]);
        let view_state = settings_providers_tab_view_state(&session);

        assert_eq!(view_state.detail.quota_visibility.len(), 2);
        // Session 应被标记为不可见（使用 stable_key 匹配，不依赖 label）
        assert_eq!(view_state.detail.quota_visibility[0].quota_key, "session");
        assert!(!view_state.detail.quota_visibility[0].visible);
        // Weekly 应仍可见
        assert_eq!(view_state.detail.quota_visibility[1].quota_key, "weekly");
        assert!(view_state.detail.quota_visibility[1].visible);
    }

    #[test]
    fn settings_detail_quota_visibility_empty_when_no_quotas() {
        let _locale_guard = setup_locale();
        let mut settings = AppSettings::default();
        settings.set_provider_enabled(ProviderKind::Claude, true);

        let provider = make_provider(ProviderKind::Claude, ConnectionStatus::Connected);
        let session = make_session(settings, pid(ProviderKind::Claude), vec![provider]);
        let view_state = settings_providers_tab_view_state(&session);

        assert!(view_state.detail.quota_visibility.is_empty());
    }

    // ── QuotaDisplayMode 透传 ────────────────────────────

    #[test]
    fn settings_detail_inherits_quota_display_mode() {
        use crate::models::QuotaDisplayMode;

        let _locale_guard = setup_locale();
        let mut settings = AppSettings::default();
        settings.set_provider_enabled(ProviderKind::Claude, true);
        settings.quota_display_mode = QuotaDisplayMode::Used;

        let session = make_session(
            settings,
            pid(ProviderKind::Claude),
            vec![make_provider(
                ProviderKind::Claude,
                ConnectionStatus::Connected,
            )],
        );

        let view_state = settings_providers_tab_view_state(&session);
        assert_eq!(view_state.detail.quota_display_mode, QuotaDisplayMode::Used);
    }
}
