//! Settings 窗口的 selector 函数
//!
//! 将 AppSession → Settings ViewModel 的转换逻辑集中于此。

use super::super::state::{AppSession, HistoryLoadState, SettingsModalState};
use super::format::{
    format_non_monitoring_message, format_provider_updated_at, format_quota_label,
    format_relative_refresh_age, format_user_failure_message, provider_source_label,
    quota_display_view_state,
};
use super::*;
use crate::history::HistoryReadyState;
use crate::models::{
    ConnectionStatus, ProviderCapability, ProviderId, ProviderKind, ProviderStatus, UpdateStatus,
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
        right_pane: settings_provider_right_pane_view_state(session),
        available_providers,
    }
}

// ── 内部 Helper ─────────────────────────────────────────────

fn settings_provider_right_pane_view_state(
    session: &AppSession,
) -> SettingsProviderRightPaneViewState {
    let form_identity = session.settings_ui.modal.form_identity();
    match &session.settings_ui.modal {
        SettingsModalState::AddingProvider => SettingsProviderRightPaneViewState::ProviderPicker,
        SettingsModalState::AddingNewApi => SettingsProviderRightPaneViewState::NewApiForm {
            identity: form_identity
                .clone()
                .expect("AddingNewApi must have form identity"),
            edit_data: None,
        },
        SettingsModalState::EditingNewApi(data) => SettingsProviderRightPaneViewState::NewApiForm {
            identity: form_identity
                .clone()
                .expect("EditingNewApi must have form identity"),
            edit_data: Some(data.clone()),
        },
        SettingsModalState::AddingScriptProvider => {
            SettingsProviderRightPaneViewState::ScriptProviderForm {
                identity: form_identity
                    .clone()
                    .expect("AddingScriptProvider must have form identity"),
                edit_data: None,
                testing: session.settings_ui.script_provider_testing,
                test_result: session.settings_ui.script_provider_test_result.clone(),
            }
        }
        SettingsModalState::EditingScriptProvider(data) => {
            SettingsProviderRightPaneViewState::ScriptProviderForm {
                identity: form_identity.expect("EditingScriptProvider must have form identity"),
                edit_data: Some(data.clone()),
                testing: session.settings_ui.script_provider_testing,
                test_result: session.settings_ui.script_provider_test_result.clone(),
            }
        }
        SettingsModalState::Idle
        | SettingsModalState::LoadingNewApi(_)
        | SettingsModalState::LoadingScriptProvider(_)
        | SettingsModalState::ConfirmingRemoveProvider
        | SettingsModalState::ConfirmingDeleteNewApi
        | SettingsModalState::ConfirmingDeleteScriptProvider
        | SettingsModalState::ConfirmingClearProviderHistory
        | SettingsModalState::ConfirmingClearAllHistory => {
            SettingsProviderRightPaneViewState::Detail
        }
    }
}

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
                    let visible = session
                        .settings
                        .provider
                        .is_quota_visible(id, &q.stable_key);
                    QuotaVisibilityItem {
                        label: format_quota_label(q),
                        quota_key: q.stable_key.clone(),
                        visible,
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    let provider_capability = provider
        .map(|p| p.provider_capability)
        .unwrap_or(ProviderCapability::Monitorable);
    let effective_rules = session.settings.effective_quota_rules(id);
    let quota_thresholds = QuotaThresholdUnit::ALL
        .iter()
        .map(|&unit| QuotaThresholdUnitViewState {
            unit,
            override_thresholds: session.settings.provider.quota_threshold_override(id, unit),
            effective: effective_rules.thresholds(unit),
        })
        .collect();

    SettingsProviderDetailViewState {
        id: id.clone(),
        icon,
        display_name,
        subtitle,
        is_enabled,
        can_refresh: provider.is_some_and(ProviderStatus::supports_refresh),
        show_quota_visibility: provider.is_some_and(ProviderStatus::supports_refresh),
        confirming_remove: session.settings_ui.modal.is_confirming_remove_provider(),
        confirming_delete_newapi: session.settings_ui.modal.is_confirming_delete_newapi(),
        confirming_delete_script_provider: session
            .settings_ui
            .modal
            .is_confirming_delete_script_provider(),
        provider_capability,
        info: settings_provider_info_view_state(provider, is_enabled),
        usage: settings_provider_usage_view_state(provider, is_enabled, &effective_rules),
        settings_capability: provider
            .map(|p| p.settings_capability.clone())
            .unwrap_or_default(),
        quota_display_mode: session.settings.display.quota_display_mode,
        quota_usage_step_pct: session.settings.provider.quota_usage_step(id),
        global_quota_usage_step_pct: session.settings.notification.quota_usage_step_pct,
        show_quota_thresholds: provider_capability == ProviderCapability::Monitorable,
        quota_thresholds,
        quota_visibility,
        history: settings_provider_history_view_state(session, id),
    }
}

fn settings_provider_history_view_state(
    session: &AppSession,
    id: &ProviderId,
) -> SettingsProviderHistoryViewState {
    SettingsProviderHistoryViewState {
        range: session.history_ui.range,
        retention_override: session.settings.provider.history_retention_days(id),
        effective_days: session.settings.effective_history_retention_days(id),
        dropdown_open: session.settings_ui.provider_history_retention_dropdown_open,
        confirming_clear: session
            .settings_ui
            .modal
            .is_confirming_clear_provider_history(),
        phase: history_phase(session, id),
    }
}

fn history_phase(session: &AppSession, id: &ProviderId) -> SettingsProviderHistoryPhase {
    let ready = match &session.history_ui.load {
        HistoryLoadState::Ready {
            provider_id, ready, ..
        } if provider_id == id => ready,
        HistoryLoadState::Unavailable { provider_id, .. } if provider_id == id => {
            return SettingsProviderHistoryPhase::Message(
                t!("provider.history.unavailable").to_string(),
            );
        }
        _ => {
            return SettingsProviderHistoryPhase::Message(
                t!("provider.history.loading").to_string(),
            );
        }
    };
    match &ready.state {
        HistoryReadyState::Empty => {
            SettingsProviderHistoryPhase::Message(t!("provider.history.empty").to_string())
        }
        HistoryReadyState::OnlyFailures { count } => SettingsProviderHistoryPhase::Message(
            t!("provider.history.only_failures", count = count).to_string(),
        ),
        HistoryReadyState::NoNumeric => {
            SettingsProviderHistoryPhase::Message(t!("provider.history.no_numeric").to_string())
        }
        HistoryReadyState::Charts(_) => {
            SettingsProviderHistoryPhase::Charts(super::history::history_charts_view(ready))
        }
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
                .map(format_provider_updated_at)
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
                ConnectionStatus::Connected
                    if provider.update_status == Some(UpdateStatus::Failed) =>
                {
                    // 数据仍在展示（陈旧旧值），但最近一次刷新失败：
                    // 状态行必须降为错误，不能继续绿色「运行中」冒充正常
                    (
                        t!("provider.status.update_failed").to_string(),
                        SettingsProviderStatusKind::Error,
                    )
                }
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
    quota_rules: &crate::models::QuotaRules,
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
                .map(|quota| quota_display_view_state(quota, quota_rules))
                .collect(),
        };
    }

    if provider.connection == ConnectionStatus::Error {
        return SettingsProviderUsageViewState::Error {
            title: t!("provider.last_fetch_failed", name = provider.display_name()).to_string(),
            message: provider
                .last_failure
                .as_ref()
                .map(format_user_failure_message)
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

    let source = provider_source_label(provider);
    match provider.connection {
        ConnectionStatus::Error => t!("provider.detail.last_failed", source = source).to_string(),
        ConnectionStatus::Refreshing => {
            t!("provider.detail.refreshing", source = source).to_string()
        }
        ConnectionStatus::Connected => {
            if let Some(instant) = provider.last_refreshed_instant {
                let time = format_relative_refresh_age(instant.elapsed().as_secs());
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
