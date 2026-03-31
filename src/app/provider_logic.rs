/// Pure formatting and business logic, free of any UI dependencies.
/// Extracted for testability (GPUI proc macros crash during test compilation).
use crate::models::{ConnectionStatus, ProviderStatus, QuotaInfo};
use rust_i18n::t;

#[allow(dead_code)]
pub fn format_amount(value: f64) -> String {
    if (value.fract() - 0.0).abs() < f64::EPSILON {
        format!("{:.0}", value)
    } else {
        format!("{:.1}", value)
    }
}

#[allow(dead_code)]
pub fn format_quota_usage(quota: &QuotaInfo) -> String {
    if quota.is_percentage_mode() {
        t!(
            "provider.remaining_pct",
            pct = format_amount(quota.limit - quota.used)
        )
        .to_string()
    } else {
        t!(
            "provider.used_of_total",
            used = format_amount(quota.used),
            total = format_amount(quota.limit)
        )
        .to_string()
    }
}

pub fn provider_empty_message(provider: &ProviderStatus) -> String {
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

#[allow(dead_code)]
pub fn provider_account_label(provider: &ProviderStatus, compact: bool) -> String {
    if let Some(email) = &provider.account_email {
        return email.clone();
    }

    if compact {
        provider.brand_name().to_string()
    } else {
        provider.account_hint().to_string()
    }
}

#[allow(dead_code)]
pub fn provider_list_subtitle(provider: &ProviderStatus) -> String {
    if !provider.enabled {
        return t!("provider.disabled_source", source = provider.source_label()).to_string();
    }
    match provider.connection {
        ConnectionStatus::Connected => {
            if let Some(ref email) = provider.account_email {
                email.clone()
            } else {
                provider.source_label().to_string()
            }
        }
        ConnectionStatus::Disconnected => t!(
            "provider.source_not_detected",
            source = provider.source_label()
        )
        .to_string(),
        ConnectionStatus::Refreshing => t!("provider.refreshing_label").to_string(),
        ConnectionStatus::Error => {
            let base = provider.source_label();
            if provider.error_message.is_some() {
                t!("provider.source_last_failed", source = base).to_string()
            } else {
                t!("provider.source_failed", source = base).to_string()
            }
        }
    }
}

pub fn provider_detail_subtitle(provider: &ProviderStatus) -> String {
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
    use crate::models::{
        ConnectionStatus, ProviderKind, ProviderMetadata, ProviderStatus, QuotaInfo,
    };

    /// 确保 i18n 测试使用英语 locale
    fn setup_locale() {
        rust_i18n::set_locale("en");
    }

    fn make_provider(kind: ProviderKind, connection: ConnectionStatus) -> ProviderStatus {
        ProviderStatus {
            kind,
            metadata: ProviderMetadata {
                kind,
                display_name: format!("{:?}", kind),
                brand_name: "Test Brand".to_string(),
                source_label: "test source".to_string(),
                account_hint: "test account".to_string(),
                icon_asset: "src/icons/usage.svg".to_string(),
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
            last_refreshed_instant: None,
        }
    }

    // ── format_amount ────────────────────────────────────────

    #[test]
    fn format_amount_integer() {
        assert_eq!(format_amount(100.0), "100");
        assert_eq!(format_amount(0.0), "0");
    }

    #[test]
    fn format_amount_decimal() {
        assert_eq!(format_amount(3.5), "3.5");
        assert_eq!(format_amount(99.9), "99.9");
    }

    // ── format_quota_usage ───────────────────────────────────

    #[test]
    fn format_quota_usage_integers() {
        setup_locale();
        let q = QuotaInfo::new("Daily", 50.0, 200.0);
        assert_eq!(format_quota_usage(&q), "50 / 200 used");
    }

    #[test]
    fn format_quota_usage_percentage_mode() {
        setup_locale();
        let q = QuotaInfo::new("Model", 65.0, 100.0);
        assert_eq!(format_quota_usage(&q), "35% remaining");
    }

    #[test]
    fn format_quota_usage_decimals() {
        setup_locale();
        let q = QuotaInfo::new("Session", 3.5, 10.0);
        assert_eq!(format_quota_usage(&q), "3.5 / 10 used");
    }

    // ── provider_account_label ───────────────────────────────

    #[test]
    fn account_label_with_email() {
        let mut p = make_provider(ProviderKind::Claude, ConnectionStatus::Connected);
        p.account_email = Some("user@example.com".into());
        assert_eq!(provider_account_label(&p, true), "user@example.com");
        assert_eq!(provider_account_label(&p, false), "user@example.com");
    }

    #[test]
    fn account_label_compact_without_email() {
        let mut p = make_provider(ProviderKind::Copilot, ConnectionStatus::Connected);
        p.metadata.brand_name = "GitHub".to_string();
        assert_eq!(provider_account_label(&p, true), "GitHub");
    }

    #[test]
    fn account_label_verbose_without_email() {
        let mut p = make_provider(ProviderKind::Copilot, ConnectionStatus::Connected);
        p.metadata.account_hint = "GitHub account".to_string();
        assert_eq!(provider_account_label(&p, false), "GitHub account");
    }

    #[test]
    fn account_label_all_providers_compact() {
        let cases = [
            (ProviderKind::Claude, "Anthropic"),
            (ProviderKind::Gemini, "Google"),
            (ProviderKind::Copilot, "GitHub"),
            (ProviderKind::Codex, "OpenAI"),
            (ProviderKind::Kimi, "Moonshot"),
            (ProviderKind::Amp, "Amp CLI"),
        ];
        for (kind, expected) in cases {
            let mut p = make_provider(kind, ConnectionStatus::Connected);
            p.metadata.brand_name = expected.to_string();
            assert_eq!(provider_account_label(&p, true), expected);
        }
    }

    // ── provider_empty_message ───────────────────────────────

    #[test]
    fn empty_message_connected() {
        setup_locale();
        let p = make_provider(ProviderKind::Claude, ConnectionStatus::Connected);
        assert_eq!(
            provider_empty_message(&p),
            "No usage details available yet."
        );
    }

    #[test]
    fn empty_message_disconnected() {
        setup_locale();
        let p = make_provider(ProviderKind::Gemini, ConnectionStatus::Disconnected);
        assert_eq!(
            provider_empty_message(&p),
            "Connect Gemini to start tracking quota."
        );
    }

    #[test]
    fn empty_message_error() {
        setup_locale();
        let p = make_provider(ProviderKind::Copilot, ConnectionStatus::Error);
        assert_eq!(
            provider_empty_message(&p),
            "Copilot usage could not be refreshed right now."
        );
    }

    #[test]
    fn empty_message_missing_env_var() {
        let mut p = make_provider(ProviderKind::Copilot, ConnectionStatus::Error);
        p.error_message = Some("配置缺失: GITHUB_TOKEN".into());
        // 错误消息直接返回，不再尝试分类
        assert_eq!(provider_empty_message(&p), "配置缺失: GITHUB_TOKEN");
    }

    #[test]
    fn empty_message_session_expired() {
        let mut p = make_provider(ProviderKind::Claude, ConnectionStatus::Error);
        p.error_message = Some("登录已过期: 请重新登录".into());
        // 错误消息直接返回，不再尝试分类
        assert_eq!(provider_empty_message(&p), "登录已过期: 请重新登录");
    }

    #[test]
    fn empty_message_generic_error() {
        let mut p = make_provider(ProviderKind::Claude, ConnectionStatus::Error);
        p.error_message = Some("network timeout".into());
        assert_eq!(provider_empty_message(&p), "network timeout");
    }
}
