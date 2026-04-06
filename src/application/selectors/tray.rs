//! Tray 弹出窗口的 selector 函数
//!
//! 将 AppSession → Tray ViewModel 的转换逻辑集中于此。

use super::format::format_last_updated;
use super::*;
use crate::app_state::{provider_panel_flags, AppSession};
use crate::models::{AppSettings, ConnectionStatus, ErrorKind, NavTab, ProviderId, ProviderStatus};
use rust_i18n::t;

pub fn header_view_state(session: &AppSession) -> HeaderViewState {
    let (status_text, status_kind) = session.header_status_text();
    HeaderViewState {
        status_text,
        status_kind,
    }
}

pub fn tray_global_actions_view_state(session: &AppSession) -> GlobalActionsViewState {
    let id = match &session.nav.active_tab {
        NavTab::Provider(id) => Some(id.clone()),
        NavTab::Settings => None,
    };

    let is_refreshing = id
        .as_ref()
        .and_then(|id| {
            session
                .provider_store
                .find_by_id(id)
                .map(|provider| provider.connection == ConnectionStatus::Refreshing)
        })
        .unwrap_or(false);

    let label = if is_refreshing {
        t!("provider.status.refreshing").to_string()
    } else {
        t!("tooltip.refresh").to_string()
    };

    GlobalActionsViewState {
        show_refresh: session.settings.display.show_refresh_button,
        refresh: RefreshButtonViewState {
            id,
            is_refreshing,
            label,
        },
    }
}

pub fn provider_detail_view_state(
    session: &AppSession,
    id: &ProviderId,
) -> ProviderDetailViewState {
    let is_enabled = session.settings.is_enabled(id);
    let provider = session.provider_store.find_by_id(id).cloned();

    if !is_enabled {
        let (icon, display_name) = if let Some(provider) = provider {
            (
                provider.icon_asset().to_string(),
                provider.display_name().to_string(),
            )
        } else {
            (
                "src/icons/provider-unknown.svg".to_string(),
                format!("{}", id),
            )
        };

        return ProviderDetailViewState::Disabled(DisabledProviderViewState {
            id: id.clone(),
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

    let flags = provider_panel_flags(&session.settings, &provider);

    let account = if flags.show_account_info {
        provider
            .account_email
            .as_ref()
            .map(|email| AccountInfoViewState {
                email: email.clone(),
                tier: provider.account_tier.clone(),
                updated_text: format_last_updated(&provider),
                dashboard_url: if flags.has_dashboard_url {
                    provider.dashboard_url().to_string()
                } else {
                    String::new()
                },
            })
    } else {
        None
    };

    let show_dashboard = flags.show_dashboard_row;

    let body = provider_body_view_state(&session.settings, session.nav.generation, &provider);

    ProviderDetailViewState::Panel(ProviderPanelViewState {
        id: id.clone(),
        show_dashboard,
        account,
        body,
        quota_display_mode: session.settings.display.quota_display_mode,
    })
}

// ── 内部 Helper ─────────────────────────────────────────────

/// Provider body 区域的状态判定
///
/// 优先级：
/// 1. 错误且无缓存配额 → Empty（展示错误信息）
/// 2. 正在刷新 → Refreshing
/// 3. 有可见配额（含错误时的缓存配额）→ Quotas
/// 4. 兜底 → Empty
fn provider_body_view_state(
    settings: &AppSettings,
    generation: u64,
    provider: &ProviderStatus,
) -> ProviderBodyViewState {
    match provider.connection {
        ConnectionStatus::Error if provider.quotas.is_empty() => {
            ProviderBodyViewState::Empty(provider_empty_view_state(provider))
        }
        ConnectionStatus::Refreshing => ProviderBodyViewState::Refreshing {
            provider_name: provider.display_name().to_string(),
        },
        _ => {
            let visible: Vec<_> = settings
                .visible_quotas(provider.provider_id.kind(), &provider.quotas)
                .into_iter()
                .cloned()
                .collect();
            if visible.is_empty() {
                ProviderBodyViewState::Empty(provider_empty_view_state(provider))
            } else {
                ProviderBodyViewState::Quotas {
                    quotas: visible,
                    generation,
                }
            }
        }
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
        id: provider.provider_id.clone(),
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

#[cfg(test)]
#[path = "tray_tests.rs"]
mod tests;
