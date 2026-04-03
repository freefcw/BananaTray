//! 格式化与展示文案函数
//!
//! 将 Provider/Quota → 展示文本 的转换逻辑集中于此。
//! 从原 `app/provider_logic.rs` 合并而来。

use crate::models::{ConnectionStatus, ProviderStatus, QuotaInfo};
use rust_i18n::t;

/// 格式化数值：整数不带小数点，非整数保留一位
pub fn format_amount(value: f64) -> String {
    if (value.fract() - 0.0).abs() < f64::EPSILON {
        format!("{:.0}", value)
    } else {
        format!("{:.1}", value)
    }
}

/// 格式化配额使用情况（用于 UI 展示）
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

/// 生成 Provider 账号标签（优先显示邮箱，否则显示品牌名/账号提示）
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

/// 生成 Provider 列表副标题（连接状态相关的描述文案）
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::test_helpers::{
        make_test_provider as make_provider, setup_test_locale as setup_locale,
    };
    use crate::models::{ConnectionStatus, ProviderKind, QuotaInfo};

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
        let _locale_guard = setup_locale();
        let q = QuotaInfo::new("Daily", 50.0, 200.0);
        assert_eq!(format_quota_usage(&q), "50 / 200 used");
    }

    #[test]
    fn format_quota_usage_percentage_mode() {
        let _locale_guard = setup_locale();
        let q = QuotaInfo::new("Model", 65.0, 100.0);
        assert_eq!(format_quota_usage(&q), "35% remaining");
    }

    #[test]
    fn format_quota_usage_decimals() {
        let _locale_guard = setup_locale();
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
}
