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
mod tests {
    use super::*;
    use crate::models::test_helpers::{
        make_test_provider as make_provider, setup_test_locale as setup_locale,
    };
    use crate::models::{AppSettings, ConnectionStatus, ProviderKind, QuotaInfo};

    fn pid(kind: ProviderKind) -> ProviderId {
        ProviderId::BuiltIn(kind)
    }

    fn make_session_with_provider(settings: AppSettings, provider: ProviderStatus) -> AppSession {
        let id = provider.provider_id.clone();
        let session = AppSession::new(settings, vec![provider]);
        assert_eq!(session.nav.active_tab, NavTab::Provider(id));
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
        settings.display.show_account_info = true;
        settings.display.show_dashboard_button = true;

        let mut provider = make_provider(ProviderKind::Gemini, ConnectionStatus::Connected);
        provider.account_email = Some("test@example.com".to_string());
        provider.account_tier = Some("Pro".to_string());
        provider.quotas = vec![QuotaInfo::new("test", 50.0, 100.0)];

        let session = make_session_with_provider(settings, provider);
        let view = provider_detail_view_state(&session, &pid(ProviderKind::Gemini));

        match view {
            ProviderDetailViewState::Panel(panel) => {
                let account = panel.account.expect("account should be Some");
                assert_eq!(account.email, "test@example.com");
                assert_eq!(account.tier, Some("Pro".to_string()));
                assert!(!account.dashboard_url.is_empty());
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
        settings.display.show_account_info = true;
        settings.display.show_dashboard_button = true;

        let mut provider = make_provider(ProviderKind::Gemini, ConnectionStatus::Connected);
        provider.quotas = vec![QuotaInfo::new("test", 50.0, 100.0)];

        let session = make_session_with_provider(settings, provider);
        let view = provider_detail_view_state(&session, &pid(ProviderKind::Gemini));

        match view {
            ProviderDetailViewState::Panel(panel) => {
                assert!(panel.account.is_none());
                assert!(panel.show_dashboard);
            }
            _ => panic!("expected Panel variant"),
        }
    }

    // ── provider_detail_view_state 顶层分支 ──────────────────

    #[test]
    fn detail_returns_disabled_when_provider_is_disabled() {
        let _locale_guard = setup_locale();
        let settings = AppSettings::default(); // 默认所有 provider 未启用
        let provider = make_provider(ProviderKind::Gemini, ConnectionStatus::Connected);

        let session = AppSession::new(settings, vec![provider]);
        let view = provider_detail_view_state(&session, &pid(ProviderKind::Gemini));

        assert!(
            matches!(view, ProviderDetailViewState::Disabled(ref d) if d.id == pid(ProviderKind::Gemini)),
            "expected Disabled variant"
        );
    }

    #[test]
    fn detail_prefers_disabled_over_missing_when_disabled_and_absent() {
        let _locale_guard = setup_locale();
        let settings = AppSettings::default();
        // 不注入任何 provider，但查询一个未启用的 id
        let session = AppSession::new(settings, vec![]);
        let view = provider_detail_view_state(&session, &pid(ProviderKind::Gemini));

        match view {
            ProviderDetailViewState::Disabled(d) => {
                assert_eq!(d.id, pid(ProviderKind::Gemini));
                assert!(
                    d.icon.contains("unknown"),
                    "absent provider should use unknown icon"
                );
            }
            _ => panic!("expected Disabled, not Missing"),
        }
    }

    #[test]
    fn detail_returns_missing_when_enabled_but_provider_absent() {
        let _locale_guard = setup_locale();
        let mut settings = AppSettings::default();
        settings.set_provider_enabled(ProviderKind::Gemini, true);

        let session = AppSession::new(settings, vec![]);
        let view = provider_detail_view_state(&session, &pid(ProviderKind::Gemini));

        assert!(
            matches!(view, ProviderDetailViewState::Missing { .. }),
            "expected Missing variant"
        );
    }

    // ── provider_body_view_state 分支测试 ────────────────────

    #[test]
    fn body_returns_error_empty_when_error_and_no_quotas() {
        let _locale_guard = setup_locale();
        let mut settings = AppSettings::default();
        settings.set_provider_enabled(ProviderKind::Gemini, true);

        let mut provider = make_provider(ProviderKind::Gemini, ConnectionStatus::Error);
        provider.error_message = Some("API key invalid".to_string());
        // quotas 为空 → 走 Error + empty 分支

        let session = make_session_with_provider(settings, provider);
        let view = provider_detail_view_state(&session, &pid(ProviderKind::Gemini));

        match view {
            ProviderDetailViewState::Panel(panel) => match panel.body {
                ProviderBodyViewState::Empty(e) => {
                    assert!(e.is_error);
                    assert_eq!(e.message, "API key invalid");
                }
                other => panic!("expected Empty body, got {:?}", other),
            },
            _ => panic!("expected Panel variant"),
        }
    }

    #[test]
    fn body_returns_refreshing_when_connection_is_refreshing() {
        let _locale_guard = setup_locale();
        let mut settings = AppSettings::default();
        settings.set_provider_enabled(ProviderKind::Gemini, true);

        let provider = make_provider(ProviderKind::Gemini, ConnectionStatus::Refreshing);

        let session = make_session_with_provider(settings, provider);
        let view = provider_detail_view_state(&session, &pid(ProviderKind::Gemini));

        match view {
            ProviderDetailViewState::Panel(panel) => {
                assert!(
                    matches!(panel.body, ProviderBodyViewState::Refreshing { .. }),
                    "expected Refreshing body"
                );
            }
            _ => panic!("expected Panel variant"),
        }
    }

    #[test]
    fn body_returns_quotas_when_visible_quotas_exist() {
        let _locale_guard = setup_locale();
        let mut settings = AppSettings::default();
        settings.set_provider_enabled(ProviderKind::Gemini, true);

        let mut provider = make_provider(ProviderKind::Gemini, ConnectionStatus::Connected);
        provider.quotas = vec![QuotaInfo::new("requests", 50.0, 100.0)];

        let session = make_session_with_provider(settings, provider);
        let view = provider_detail_view_state(&session, &pid(ProviderKind::Gemini));

        match view {
            ProviderDetailViewState::Panel(panel) => match panel.body {
                ProviderBodyViewState::Quotas { quotas, generation } => {
                    assert_eq!(quotas.len(), 1);
                    assert_eq!(generation, session.nav.generation);
                }
                other => panic!("expected Quotas body, got {:?}", other),
            },
            _ => panic!("expected Panel variant"),
        }
    }

    #[test]
    fn body_returns_empty_when_all_quotas_hidden() {
        let _locale_guard = setup_locale();
        let mut settings = AppSettings::default();
        settings.set_provider_enabled(ProviderKind::Gemini, true);
        // 隐藏 general 类型的配额
        settings.toggle_quota_visibility(ProviderKind::Gemini, "general".to_string());

        let mut provider = make_provider(ProviderKind::Gemini, ConnectionStatus::Connected);
        provider.quotas = vec![QuotaInfo::new("requests", 50.0, 100.0)]; // QuotaType::General

        let session = make_session_with_provider(settings, provider);
        let view = provider_detail_view_state(&session, &pid(ProviderKind::Gemini));

        match view {
            ProviderDetailViewState::Panel(panel) => match panel.body {
                ProviderBodyViewState::Empty(e) => {
                    assert!(!e.is_error, "should not be error state");
                }
                other => panic!("expected Empty body, got {:?}", other),
            },
            _ => panic!("expected Panel variant"),
        }
    }

    #[test]
    fn body_prefers_cached_quotas_over_error_empty() {
        let _locale_guard = setup_locale();
        let mut settings = AppSettings::default();
        settings.set_provider_enabled(ProviderKind::Gemini, true);

        let mut provider = make_provider(ProviderKind::Gemini, ConnectionStatus::Error);
        provider.error_message = Some("timeout".to_string());
        // 有缓存配额 → 即使出错也应展示配额而非 Empty
        provider.quotas = vec![QuotaInfo::new("requests", 50.0, 100.0)];

        let session = make_session_with_provider(settings, provider);
        let view = provider_detail_view_state(&session, &pid(ProviderKind::Gemini));

        match view {
            ProviderDetailViewState::Panel(panel) => match panel.body {
                ProviderBodyViewState::Quotas { quotas, .. } => {
                    assert_eq!(
                        quotas.len(),
                        1,
                        "cached quotas should be shown despite error"
                    );
                }
                other => panic!("expected Quotas body (cached), got {:?}", other),
            },
            _ => panic!("expected Panel variant"),
        }
    }

    // ── QuotaDisplayMode 透传 ────────────────────────────

    #[test]
    fn panel_inherits_quota_display_mode_from_settings() {
        use crate::models::QuotaDisplayMode;

        let _locale_guard = setup_locale();
        let mut settings = AppSettings::default();
        settings.set_provider_enabled(ProviderKind::Gemini, true);
        settings.display.quota_display_mode = QuotaDisplayMode::Used;

        let mut provider = make_provider(ProviderKind::Gemini, ConnectionStatus::Connected);
        provider.quotas = vec![QuotaInfo::new("test", 50.0, 100.0)];

        let session = make_session_with_provider(settings, provider);
        let view = provider_detail_view_state(&session, &pid(ProviderKind::Gemini));

        match view {
            ProviderDetailViewState::Panel(panel) => {
                assert_eq!(panel.quota_display_mode, QuotaDisplayMode::Used);
            }
            _ => panic!("expected Panel variant"),
        }
    }

    #[test]
    fn panel_defaults_to_remaining_mode() {
        let _locale_guard = setup_locale();
        let mut settings = AppSettings::default();
        settings.set_provider_enabled(ProviderKind::Gemini, true);

        let mut provider = make_provider(ProviderKind::Gemini, ConnectionStatus::Connected);
        provider.quotas = vec![QuotaInfo::new("test", 50.0, 100.0)];

        let session = make_session_with_provider(settings, provider);
        let view = provider_detail_view_state(&session, &pid(ProviderKind::Gemini));

        match view {
            ProviderDetailViewState::Panel(panel) => {
                assert_eq!(
                    panel.quota_display_mode,
                    crate::models::QuotaDisplayMode::Remaining
                );
            }
            _ => panic!("expected Panel variant"),
        }
    }
}
