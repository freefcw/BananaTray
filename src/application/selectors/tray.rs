//! Tray 弹出窗口的 selector 函数
//!
//! 将 AppSession → Tray ViewModel 的转换逻辑集中于此。

use super::super::state::{provider_panel_flags, AppSession};
use super::format::{
    format_failure_message, format_last_updated, format_quota_label, quota_display_view_state,
};
use super::*;
use crate::models::{AppSettings, ConnectionStatus, ErrorKind, NavTab, ProviderId, ProviderStatus};
use rust_i18n::t;

pub fn header_view_state(session: &AppSession) -> HeaderViewState {
    let (status_kind, elapsed) = session.header_status_text();
    let status_text = match status_kind {
        HeaderStatusKind::Synced => t!("header.synced").to_string(),
        HeaderStatusKind::Syncing => t!("header.syncing").to_string(),
        HeaderStatusKind::Offline => t!("header.offline").to_string(),
        HeaderStatusKind::Stale => {
            let secs = elapsed.unwrap_or(0);
            if secs < 3600 {
                t!("header.minutes_ago", n = secs / 60).to_string()
            } else {
                t!("header.hours_ago", n = secs / 3600).to_string()
            }
        }
    };
    HeaderViewState {
        status_text,
        status_kind,
    }
}

pub fn tray_global_actions_view_state(session: &AppSession) -> GlobalActionsViewState {
    let target = match &session.nav.active_tab {
        NavTab::Provider(id) => Some(RefreshTarget::One(id.clone())),
        NavTab::Overview => Some(RefreshTarget::All),
        NavTab::Settings => None,
    };

    let is_refreshing = match &target {
        Some(RefreshTarget::All) => {
            // Overview 模式：任何一个已启用 Provider 正在刷新即视为 refreshing
            session.provider_store.providers.iter().any(|p| {
                session.settings.provider.is_enabled(&p.provider_id)
                    && p.connection == ConnectionStatus::Refreshing
            })
        }
        Some(RefreshTarget::One(id)) => session
            .provider_store
            .find_by_id(id)
            .is_some_and(|p| p.connection == ConnectionStatus::Refreshing),
        None => false,
    };

    let label = if is_refreshing {
        t!("provider.status.refreshing").to_string()
    } else {
        t!("tooltip.refresh").to_string()
    };

    GlobalActionsViewState {
        show_refresh: session.settings.display.show_refresh_button,
        refresh: RefreshButtonViewState {
            target,
            is_refreshing,
            label,
        },
    }
}

pub fn provider_detail_view_state(
    session: &AppSession,
    id: &ProviderId,
) -> ProviderDetailViewState {
    let is_enabled = session.settings.provider.is_enabled(id);
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
                dashboard_url: flags
                    .has_dashboard_url
                    .then(|| provider.dashboard_url().to_string()),
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
                .provider
                .visible_quotas(provider.provider_id.kind(), &provider.quotas)
                .into_iter()
                .cloned()
                .collect();
            if visible.is_empty() {
                ProviderBodyViewState::Empty(provider_empty_view_state(provider))
            } else {
                ProviderBodyViewState::Quotas {
                    quotas: visible
                        .into_iter()
                        .map(|quota| quota_display_view_state(&quota))
                        .collect(),
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
            provider
                .last_failure
                .as_ref()
                .map(format_failure_message)
                .unwrap_or_default(),
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
    if let Some(failure) = &provider.last_failure {
        return format_failure_message(failure);
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

// ── Overview 总览 ───────────────────────────────────────────

pub fn overview_view_state(session: &AppSession) -> OverviewViewState {
    let custom_ids = session.provider_store.custom_provider_ids();
    let ordered_ids = session.settings.provider.ordered_provider_ids(&custom_ids);
    let display_mode = session.settings.display.quota_display_mode;

    let items: Vec<OverviewItemViewState> = ordered_ids
        .iter()
        .filter(|id| session.settings.provider.is_enabled(id))
        .filter_map(|id| {
            let provider = session.provider_store.find_by_id(id)?;
            let icon = provider.icon_asset().to_string();
            let display_name = provider.display_name().to_string();

            let status = match provider.connection {
                ConnectionStatus::Refreshing => OverviewItemStatus::Refreshing,
                ConnectionStatus::Disconnected => OverviewItemStatus::Disconnected,
                ConnectionStatus::Error if provider.quotas.is_empty() => {
                    OverviewItemStatus::Error {
                        message: provider
                            .last_failure
                            .as_ref()
                            .map(format_failure_message)
                            .unwrap_or_else(|| t!("provider.refresh_failed").to_string()),
                    }
                }
                // Connected 或 Error（有缓存配额）：展示配额数据
                ConnectionStatus::Connected | ConnectionStatus::Error => {
                    let visible = session
                        .settings
                        .provider
                        .visible_quotas(provider.kind(), &provider.quotas);
                    if visible.is_empty() {
                        OverviewItemStatus::Disconnected
                    } else {
                        // 收集所有可见配额，按 status_level 降序（最差在前）
                        let mut quota_items: Vec<OverviewQuotaItem> = visible
                            .iter()
                            .map(|q| {
                                let sl = q.status_level();
                                OverviewQuotaItem {
                                    label: format_quota_label(q),
                                    display_text: compact_quota_display_text(q, display_mode),
                                    bar_ratio: compact_quota_bar_ratio(q, sl, display_mode),
                                    status_level: sl,
                                }
                            })
                            .collect();
                        quota_items.sort_by(|a, b| b.status_level.cmp(&a.status_level));
                        let worst = quota_items[0].status_level;
                        OverviewItemStatus::Quota {
                            status_level: worst,
                            quotas: quota_items,
                        }
                    }
                }
            };

            Some(OverviewItemViewState {
                id: id.clone(),
                icon,
                display_name,
                status,
            })
        })
        .collect();

    OverviewViewState { items }
}

/// Overview 紧凑显示文本：根据 display_mode 选择 Remaining/Used 模式
fn compact_quota_display_text(
    quota: &crate::models::QuotaInfo,
    display_mode: crate::models::QuotaDisplayMode,
) -> String {
    use crate::models::{QuotaDisplayMode, QuotaType};

    if quota.is_balance_only() {
        let balance = quota.remaining_balance.unwrap_or(0.0);
        return if matches!(quota.quota_type, QuotaType::Credit) {
            format!("${:.2}", balance)
        } else {
            format!("{:.2}", balance)
        };
    }

    match (&quota.quota_type, display_mode) {
        (QuotaType::Credit, QuotaDisplayMode::Remaining) => {
            let remaining = quota.limit - quota.used;
            if remaining >= 0.0 {
                format!("${:.2}", remaining)
            } else {
                format!("-${:.2}", -remaining)
            }
        }
        (QuotaType::Credit, QuotaDisplayMode::Used) => {
            format!("${:.2}", quota.used)
        }
        (_, QuotaDisplayMode::Remaining) => {
            format!("{:.0}%", quota.percent_remaining().max(0.0))
        }
        (_, QuotaDisplayMode::Used) => {
            format!("{:.0}%", quota.percentage().clamp(0.0, 100.0))
        }
    }
}

/// Overview 紧凑进度条比例 [0.0, 1.0]
///
/// Remaining 模式：进度条表示剩余比例（满→空）
/// Used 模式：进度条表示已用比例（空→满），与文本语义一致
fn compact_quota_bar_ratio(
    quota: &crate::models::QuotaInfo,
    level: crate::models::StatusLevel,
    display_mode: crate::models::QuotaDisplayMode,
) -> f32 {
    use crate::models::{QuotaDisplayMode, StatusLevel};

    if quota.is_balance_only() {
        // 余额模式无进度条意义，用状态等级粗略映射
        match level {
            StatusLevel::Green => 0.8,
            StatusLevel::Yellow => 0.4,
            StatusLevel::Red => 0.1,
        }
    } else {
        match display_mode {
            QuotaDisplayMode::Remaining => {
                let pct = quota.percent_remaining().clamp(0.0, 100.0);
                pct as f32 / 100.0
            }
            QuotaDisplayMode::Used => {
                let pct = quota.percentage().clamp(0.0, 100.0);
                pct as f32 / 100.0
            }
        }
    }
}

#[cfg(test)]
#[path = "tray_tests.rs"]
mod tests;
