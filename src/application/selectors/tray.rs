//! Tray 弹出窗口的 selector 函数
//!
//! 将 AppSession → Tray ViewModel 的转换逻辑集中于此。

use super::*;
use crate::app_state::AppSession;
use crate::models::{ConnectionStatus, ErrorKind, NavTab, ProviderKind, ProviderStatus};
use rust_i18n::t;

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

// ── 内部 Helper ─────────────────────────────────────────────

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
