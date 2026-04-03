//! Tray 弹出窗口的 selector 函数
//!
//! 将 AppSession → Tray ViewModel 的转换逻辑集中于此。

use super::*;
use crate::app_state::{provider_panel_flags, AppSession};
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

    let flags = provider_panel_flags(&session.settings, &provider);

    let account = if flags.show_account_info {
        provider
            .account_email
            .as_ref()
            .map(|email| AccountInfoViewState {
                email: email.clone(),
                tier: provider.account_tier.clone(),
                updated_text: provider.format_last_updated(),
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
        account,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::test_helpers::{
        make_test_provider as make_provider, setup_test_locale as setup_locale,
    };
    use crate::models::{AppSettings, ConnectionStatus, QuotaInfo};

    fn make_session_with_provider(settings: AppSettings, provider: ProviderStatus) -> AppSession {
        let kind = provider.kind;
        let session = AppSession::new(settings, vec![provider]);
        // new() 自动选择第一个启用的 provider 为 active tab
        assert!(matches!(
            session.nav.active_tab,
            NavTab::Provider(k) if k == kind
        ));
        session
    }

    // ── Account Info 冒烟测试 ─────────────────────────────────
    // 边界组合（setting off / no email / dashboard off）已在
    // app_state::tests::panel_flags_* 单元测试中覆盖，
    // 这里只验证 selector 正确集成 flags → ViewModel 的端到端路径。

    #[test]
    fn account_card_assembled_when_email_and_setting_on() {
        let _locale_guard = setup_locale();
        let mut settings = AppSettings::default();
        settings.set_provider_enabled(ProviderKind::Gemini, true);
        settings.show_account_info = true;
        settings.show_dashboard_button = true;

        let mut provider = make_provider(ProviderKind::Gemini, ConnectionStatus::Connected);
        provider.account_email = Some("test@example.com".to_string());
        provider.account_tier = Some("Pro".to_string());
        provider.quotas = vec![QuotaInfo::new("test", 50.0, 100.0)];

        let session = make_session_with_provider(settings, provider);
        let view = provider_detail_view_state(&session, ProviderKind::Gemini);

        match view {
            ProviderDetailViewState::Panel(panel) => {
                let account = panel.account.expect("account should be Some");
                assert_eq!(account.email, "test@example.com");
                assert_eq!(account.tier, Some("Pro".to_string()));
                assert!(!account.dashboard_url.is_empty());
                // 账户卡片存在时，dashboard 行应隐藏（互斥）
                assert!(!panel.show_dashboard);
            }
            _ => panic!("expected Panel variant"),
        }
    }

    #[test]
    fn no_account_card_when_email_absent() {
        let _locale_guard = setup_locale();
        let mut settings = AppSettings::default();
        settings.set_provider_enabled(ProviderKind::Gemini, true);
        settings.show_account_info = true;
        settings.show_dashboard_button = true;

        let mut provider = make_provider(ProviderKind::Gemini, ConnectionStatus::Connected);
        provider.quotas = vec![QuotaInfo::new("test", 50.0, 100.0)];

        let session = make_session_with_provider(settings, provider);
        let view = provider_detail_view_state(&session, ProviderKind::Gemini);

        match view {
            ProviderDetailViewState::Panel(panel) => {
                assert!(panel.account.is_none());
                assert!(panel.show_dashboard);
            }
            _ => panic!("expected Panel variant"),
        }
    }
}
