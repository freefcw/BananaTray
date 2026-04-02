use crate::app_state::{AppSession, HeaderStatusKind};
use crate::models::{ConnectionStatus, ErrorKind, NavTab, ProviderKind, ProviderStatus, QuotaInfo};
use rust_i18n::t;

#[derive(Debug, Clone)]
pub struct HeaderViewState {
    pub status_text: String,
    pub status_kind: HeaderStatusKind,
}

#[derive(Debug, Clone)]
pub struct GlobalActionsViewState {
    pub show_refresh: bool,
    pub refresh: RefreshButtonViewState,
}

#[derive(Debug, Clone)]
pub struct RefreshButtonViewState {
    pub kind: Option<ProviderKind>,
    pub is_refreshing: bool,
    pub label: String,
}

#[derive(Debug, Clone)]
pub enum ProviderDetailViewState {
    Disabled(DisabledProviderViewState),
    Missing { message: String },
    Panel(ProviderPanelViewState),
}

#[derive(Debug, Clone)]
pub struct DisabledProviderViewState {
    pub kind: ProviderKind,
    pub icon: String,
    pub title: String,
    pub hint: String,
}

#[derive(Debug, Clone)]
pub struct ProviderPanelViewState {
    pub kind: ProviderKind,
    pub show_dashboard: bool,
    pub body: ProviderBodyViewState,
}

#[derive(Debug, Clone)]
pub struct SettingsProvidersTabViewState {
    pub items: Vec<SettingsProviderListItemViewState>,
    pub detail: SettingsProviderDetailViewState,
}

#[derive(Debug, Clone)]
pub struct SettingsProviderListItemViewState {
    pub kind: ProviderKind,
    pub icon: String,
    pub display_name: String,
    pub is_selected: bool,
    pub is_enabled: bool,
    pub can_move_up: bool,
    pub can_move_down: bool,
}

#[derive(Debug, Clone)]
pub struct SettingsProviderDetailViewState {
    pub kind: ProviderKind,
    pub icon: String,
    pub display_name: String,
    pub subtitle: String,
    pub is_enabled: bool,
    pub info: SettingsProviderInfoViewState,
    pub usage: SettingsProviderUsageViewState,
    pub settings_mode: ProviderSettingsMode,
}

#[derive(Debug, Clone)]
pub struct SettingsProviderInfoViewState {
    pub state_text: String,
    pub source_text: String,
    pub updated_text: String,
    pub status_text: String,
    pub status_kind: SettingsProviderStatusKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsProviderStatusKind {
    Neutral,
    Success,
    Error,
}

#[derive(Debug, Clone)]
pub enum SettingsProviderUsageViewState {
    Disabled { message: String },
    Quotas { quotas: Vec<QuotaInfo> },
    Error { title: String, message: String },
    Empty { message: String },
    Missing { message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderSettingsMode {
    AutoManaged,
    Interactive,
}

#[derive(Debug, Clone)]
pub enum ProviderBodyViewState {
    Refreshing {
        provider_name: String,
    },
    Quotas {
        quotas: Vec<QuotaInfo>,
        generation: u64,
    },
    Empty(ProviderEmptyViewState),
}

#[derive(Debug, Clone)]
pub struct ProviderEmptyViewState {
    pub kind: ProviderKind,
    pub title: String,
    pub message: String,
    pub is_error: bool,
    pub action: Option<ProviderEmptyAction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderEmptyAction {
    OpenSettings,
    RetryRefresh,
}

pub fn header_view_state(session: &AppSession) -> HeaderViewState {
    let (status_text, status_kind) = session.header_status_text();
    HeaderViewState {
        status_text,
        status_kind,
    }
}

pub fn tray_global_actions_view_state(session: &AppSession) -> GlobalActionsViewState {
    let kind = match session.nav.active_tab {
        NavTab::Provider(kind) => Some(kind),
        NavTab::Settings => None,
    };

    let is_refreshing = kind
        .and_then(|kind| {
            session
                .provider_store
                .find(kind)
                .map(|provider| provider.connection == ConnectionStatus::Refreshing)
        })
        .unwrap_or(false);

    let label = if is_refreshing {
        t!("provider.status.refreshing").to_string()
    } else {
        t!("tooltip.refresh").to_string()
    };

    GlobalActionsViewState {
        show_refresh: session.settings.show_refresh_button,
        refresh: RefreshButtonViewState {
            kind,
            is_refreshing,
            label,
        },
    }
}

pub fn provider_detail_view_state(
    session: &AppSession,
    kind: ProviderKind,
) -> ProviderDetailViewState {
    let is_enabled = session.settings.is_provider_enabled(kind);
    let provider = session.provider_store.find(kind).cloned();

    if !is_enabled {
        let (icon, display_name) = if let Some(provider) = provider {
            (
                provider.icon_asset().to_string(),
                provider.display_name().to_string(),
            )
        } else {
            (
                "src/icons/provider-unknown.svg".to_string(),
                format!("{:?}", kind),
            )
        };

        return ProviderDetailViewState::Disabled(DisabledProviderViewState {
            kind,
            icon,
            title: t!("provider.not_enabled", name = display_name).to_string(),
            hint: t!("provider.enable_hint").to_string(),
        });
    }

    let Some(provider) = provider else {
        return ProviderDetailViewState::Missing {
            message: t!("provider.not_found").to_string(),
        };
    };

    let show_dashboard =
        session.settings.show_dashboard_button && !provider.dashboard_url().is_empty();
    let is_refreshing = provider.connection == ConnectionStatus::Refreshing;
    let is_error = provider.connection == ConnectionStatus::Error;
    let has_quotas = !provider.quotas.is_empty();

    let body = if is_error && !has_quotas {
        ProviderBodyViewState::Empty(provider_empty_view_state(&provider))
    } else if is_refreshing {
        ProviderBodyViewState::Refreshing {
            provider_name: provider.display_name().to_string(),
        }
    } else if has_quotas {
        ProviderBodyViewState::Quotas {
            quotas: provider.quotas.clone(),
            generation: session.nav.generation,
        }
    } else {
        ProviderBodyViewState::Empty(provider_empty_view_state(&provider))
    };

    ProviderDetailViewState::Panel(ProviderPanelViewState {
        kind,
        show_dashboard,
        body,
    })
}

pub fn settings_providers_tab_view_state(session: &AppSession) -> SettingsProvidersTabViewState {
    let ordered = session.settings.ordered_providers();
    let selected = session.settings_ui.selected_provider;

    let items = ordered
        .iter()
        .enumerate()
        .map(|(index, kind)| {
            let provider = session.provider_store.find(*kind);
            SettingsProviderListItemViewState {
                kind: *kind,
                icon: provider
                    .map(|provider| provider.icon_asset().to_string())
                    .unwrap_or_else(|| "src/icons/provider-unknown.svg".to_string()),
                display_name: provider
                    .map(|provider| provider.display_name().to_string())
                    .unwrap_or_else(|| format!("{:?}", kind)),
                is_selected: *kind == selected,
                is_enabled: session.settings.is_provider_enabled(*kind),
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

fn settings_provider_detail_view_state(
    session: &AppSession,
    kind: ProviderKind,
) -> SettingsProviderDetailViewState {
    let provider = session.provider_store.find(kind);
    let is_enabled = session.settings.is_provider_enabled(kind);

    let (icon, display_name, subtitle) = if let Some(provider) = provider {
        (
            provider.icon_asset().to_string(),
            provider.display_name().to_string(),
            settings_provider_subtitle(provider),
        )
    } else {
        (
            "src/icons/provider-unknown.svg".to_string(),
            format!("{:?}", kind),
            format!("{:?} · {}", kind, t!("provider.not_available")),
        )
    };

    SettingsProviderDetailViewState {
        kind,
        icon,
        display_name,
        subtitle,
        is_enabled,
        info: settings_provider_info_view_state(provider, is_enabled),
        usage: settings_provider_usage_view_state(provider, is_enabled),
        settings_mode: match kind {
            ProviderKind::Copilot => ProviderSettingsMode::Interactive,
            _ => ProviderSettingsMode::AutoManaged,
        },
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
        .map(|provider| provider.format_last_updated())
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

fn provider_empty_view_state(provider: &ProviderStatus) -> ProviderEmptyViewState {
    let is_error = provider.connection == ConnectionStatus::Error;
    let is_config_error = matches!(
        provider.error_kind,
        ErrorKind::ConfigMissing | ErrorKind::AuthRequired
    );

    let (title, message) = if is_error {
        (
            t!("provider.refresh_failed").to_string(),
            provider.error_message.clone().unwrap_or_default(),
        )
    } else {
        let title = match provider.connection {
            ConnectionStatus::Connected => t!("provider.waiting").to_string(),
            ConnectionStatus::Refreshing => t!("provider.status.refreshing").to_string(),
            ConnectionStatus::Disconnected => t!("provider.connection_required").to_string(),
            ConnectionStatus::Error => unreachable!(),
        };
        (title, provider_empty_message(provider))
    };

    let action = match provider.connection {
        ConnectionStatus::Error | ConnectionStatus::Disconnected => {
            if is_config_error {
                Some(ProviderEmptyAction::OpenSettings)
            } else {
                Some(ProviderEmptyAction::RetryRefresh)
            }
        }
        _ => None,
    };

    ProviderEmptyViewState {
        kind: provider.kind,
        title,
        message,
        is_error,
        action,
    }
}

fn provider_empty_message(provider: &ProviderStatus) -> String {
    if let Some(error) = &provider.error_message {
        return error.clone();
    }

    match provider.connection {
        ConnectionStatus::Error => {
            t!("provider.cannot_refresh", name = provider.display_name()).to_string()
        }
        ConnectionStatus::Refreshing => {
            t!("provider.fetching", name = provider.display_name()).to_string()
        }
        ConnectionStatus::Disconnected => {
            t!("provider.connect_to_track", name = provider.display_name()).to_string()
        }
        ConnectionStatus::Connected => t!("provider.no_usage_details").to_string(),
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
                let time = provider.format_last_updated().to_lowercase();
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
    use crate::models::{AppSettings, ErrorKind, ProviderMetadata, ProviderStatus};

    fn setup_locale() {
        rust_i18n::set_locale("en");
    }

    fn make_provider(kind: ProviderKind, connection: ConnectionStatus) -> ProviderStatus {
        ProviderStatus {
            kind,
            metadata: ProviderMetadata {
                kind,
                display_name: format!("{:?}", kind),
                brand_name: format!("{:?}", kind),
                source_label: "CLI".to_string(),
                account_hint: "account".to_string(),
                icon_asset: "src/icons/provider.svg".to_string(),
                dashboard_url: "https://example.com".to_string(),
            },
            enabled: true,
            connection,
            quotas: vec![],
            account_email: None,
            is_paid: false,
            account_tier: None,
            last_updated_at: None,
            error_message: None,
            error_kind: ErrorKind::default(),
            last_refreshed_instant: None,
        }
    }

    fn make_session(
        settings: AppSettings,
        selected_provider: ProviderKind,
        providers: Vec<ProviderStatus>,
    ) -> AppSession {
        let mut session = AppSession::new(settings, providers);
        session.settings_ui.selected_provider = selected_provider;
        session
    }

    #[test]
    fn settings_providers_tab_marks_reorder_boundaries() {
        setup_locale();
        let mut settings = AppSettings::default();
        settings.provider_order = vec!["gemini".into(), "claude".into(), "copilot".into()];
        settings.set_provider_enabled(ProviderKind::Gemini, true);
        settings.set_provider_enabled(ProviderKind::Claude, true);
        settings.set_provider_enabled(ProviderKind::Copilot, true);

        let session = make_session(
            settings,
            ProviderKind::Claude,
            vec![
                make_provider(ProviderKind::Gemini, ConnectionStatus::Connected),
                make_provider(ProviderKind::Claude, ConnectionStatus::Connected),
                make_provider(ProviderKind::Copilot, ConnectionStatus::Connected),
            ],
        );

        let view_state = settings_providers_tab_view_state(&session);

        assert_eq!(view_state.items[0].kind, ProviderKind::Gemini);
        assert!(!view_state.items[0].can_move_up);
        assert!(view_state.items[0].can_move_down);
        assert_eq!(view_state.items[1].kind, ProviderKind::Claude);
        assert!(view_state.items[1].is_selected);
        assert!(view_state.items[1].can_move_up);
        assert!(view_state.items[1].can_move_down);
    }

    #[test]
    fn settings_provider_detail_reports_disabled_usage() {
        setup_locale();
        let mut settings = AppSettings::default();
        settings.set_provider_enabled(ProviderKind::Claude, false);

        let session = make_session(
            settings,
            ProviderKind::Claude,
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
        setup_locale();
        let mut settings = AppSettings::default();
        settings.set_provider_enabled(ProviderKind::Copilot, true);

        let mut provider = make_provider(ProviderKind::Copilot, ConnectionStatus::Error);
        provider.error_message = Some("boom".to_string());

        let session = make_session(settings, ProviderKind::Copilot, vec![provider]);
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
}
