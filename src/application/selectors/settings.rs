//! Settings 窗口的 selector 函数
//!
//! 将 AppSession → Settings ViewModel 的转换逻辑集中于此。

use super::super::state::AppSession;
use super::format::{
    format_failure_message, format_last_updated, format_non_monitoring_message, format_quota_label,
    quota_display_view_state,
};
use super::*;
use crate::models::{
    ConnectionStatus, ProviderCapability, ProviderId, ProviderKind, ProviderStatus,
};
use rust_i18n::t;

pub fn settings_providers_tab_view_state(session: &AppSession) -> SettingsProvidersTabViewState {
    let custom_ids = session.provider_store.custom_provider_ids();
    // 核心变更：仅展示 sidebar 中的 Provider（动态子集）
    let ordered = session.settings.provider.sidebar_provider_ids(&custom_ids);
    let selected = &session.settings_ui.selected_provider;

    let items = ordered
        .iter()
        .map(|id| {
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
                is_enabled: session.settings.provider.is_enabled(id),
            }
        })
        .collect();

    // 计算可添加的 Provider 列表
    let manager_metadata = |kind: ProviderKind| -> (String, String) {
        session
            .provider_store
            .providers
            .iter()
            .find(|p| p.provider_id == ProviderId::BuiltIn(kind))
            .map(|p| (p.icon_asset().to_string(), p.display_name().to_string()))
            .unwrap_or_else(|| {
                (
                    "src/icons/provider-unknown.svg".to_string(),
                    format!("{:?}", kind),
                )
            })
    };

    let available_providers = session
        .settings
        .provider
        .addable_provider_kinds()
        .into_iter()
        .map(|kind| {
            let (icon, display_name) = manager_metadata(kind);
            AvailableProviderItem {
                id: ProviderId::BuiltIn(kind),
                icon,
                display_name,
            }
        })
        .collect();

    SettingsProvidersTabViewState {
        items,
        detail: settings_provider_detail_view_state(session, selected),
        adding_newapi: session.settings_ui.adding_newapi,
        editing_newapi_data: session.settings_ui.editing_newapi.clone(),
        adding_provider: session.settings_ui.adding_provider,
        available_providers,
    }
}

// ── 内部 Helper ─────────────────────────────────────────────

fn settings_provider_detail_view_state(
    session: &AppSession,
    id: &ProviderId,
) -> SettingsProviderDetailViewState {
    let provider = session.provider_store.find_by_id(id);
    let is_enabled = session.settings.provider.is_enabled(id);

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
                    let quota_key = q.stable_key.clone();
                    QuotaVisibilityItem {
                        label: format_quota_label(q),
                        quota_key: quota_key.clone(),
                        visible: session
                            .settings
                            .provider
                            .is_quota_visible(id.kind(), &quota_key),
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
        can_refresh: provider.is_some_and(ProviderStatus::supports_refresh),
        show_quota_visibility: provider.is_some_and(ProviderStatus::supports_refresh),
        provider_capability: provider
            .map(|p| p.provider_capability)
            .unwrap_or(ProviderCapability::Monitorable),
        info: settings_provider_info_view_state(provider, is_enabled),
        usage: settings_provider_usage_view_state(provider, is_enabled),
        settings_capability: provider
            .map(|p| p.settings_capability.clone())
            .unwrap_or_default(),
        quota_display_mode: session.settings.display.quota_display_mode,
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
    let source_text = provider
        .filter(|provider| !provider.supports_refresh())
        .map(|_| t!("provider.source.reference").to_string())
        .unwrap_or_else(|| t!("provider.source.auto").to_string());
    let updated_text = provider
        .filter(|provider| !provider.supports_refresh())
        .map(|_| t!("provider.not_applicable").to_string())
        .unwrap_or_else(|| {
            provider
                .map(format_last_updated)
                .unwrap_or_else(|| t!("provider.not_fetched").to_string())
        });

    let (status_text, status_kind) = provider
        .map(|provider| {
            if !provider.supports_refresh() {
                let label = match provider.provider_capability {
                    ProviderCapability::Informational => t!("provider.status.reference_only"),
                    ProviderCapability::Placeholder => t!("provider.status.not_monitorable"),
                    ProviderCapability::Monitorable => unreachable!(),
                }
                .to_string();
                return (label, SettingsProviderStatusKind::Neutral);
            }

            match provider.connection {
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
            }
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

    if !provider.supports_refresh() {
        return SettingsProviderUsageViewState::Empty {
            message: format_non_monitoring_message(provider),
        };
    }

    if !provider.quotas.is_empty() {
        return SettingsProviderUsageViewState::Quotas {
            quotas: provider
                .quotas
                .iter()
                .map(quota_display_view_state)
                .collect(),
        };
    }

    if provider.connection == ConnectionStatus::Error {
        return SettingsProviderUsageViewState::Error {
            title: t!("provider.last_fetch_failed", name = provider.display_name()).to_string(),
            message: provider
                .last_failure
                .as_ref()
                .map(format_failure_message)
                .unwrap_or_else(|| t!("provider.unknown_error").to_string()),
        };
    }

    SettingsProviderUsageViewState::Empty {
        message: t!("provider.no_usage").to_string(),
    }
}

fn settings_provider_subtitle(provider: &ProviderStatus) -> String {
    if !provider.supports_refresh() {
        return match provider.provider_capability {
            ProviderCapability::Informational => t!("provider.detail.reference_only").to_string(),
            ProviderCapability::Placeholder => t!("provider.detail.not_monitorable").to_string(),
            ProviderCapability::Monitorable => unreachable!(),
        };
    }

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
#[path = "settings_tests.rs"]
mod tests;
