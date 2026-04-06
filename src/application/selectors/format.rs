//! 格式化与展示文案函数
//!
//! 将 Provider/Quota → 展示文本 的转换逻辑集中于此。
//! 从原 `app/provider_logic.rs` 合并而来。

use crate::models::{ConnectionStatus, ProviderStatus, QuotaInfo, QuotaType, UpdateStatus};
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

/// 格式化上次刷新的相对时间
///
/// 从 `ProviderStatus` 的实例方法提取到 selector 层，
/// 消除数据模型对 i18n 的依赖（DIP 原则）。
pub fn format_last_updated(provider: &ProviderStatus) -> String {
    if let Some(instant) = provider.last_refreshed_instant {
        let secs = instant.elapsed().as_secs();
        if secs < 60 {
            t!("provider.updated_just_now").to_string()
        } else if secs < 3600 {
            t!("provider.updated_min_ago", n = secs / 60).to_string()
        } else {
            t!("provider.updated_hr_ago", n = secs / 3600).to_string()
        }
    } else if let Some(status) = provider.update_status {
        match status {
            UpdateStatus::Failed => t!("quota.update_failed").to_string(),
        }
    } else {
        match provider.connection {
            ConnectionStatus::Connected => t!("provider.waiting_for_data").to_string(),
            ConnectionStatus::Refreshing => t!("provider.status.refreshing").to_string(),
            ConnectionStatus::Error => t!("provider.needs_attention").to_string(),
            ConnectionStatus::Disconnected => t!("provider.not_connected").to_string(),
        }
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
pub fn provider_list_subtitle(provider: &ProviderStatus, enabled: bool) -> String {
    if !enabled {
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

/// 剩余量摘要文本（用于 UI 主显示）
///
/// 从 `QuotaInfo` 的实例方法提取到 selector 层，
/// 消除数据模型对 i18n 的依赖（DIP 原则）。
///
/// - 余额模式: "$X.XX" 或 "X.XX"（直接显示余额数值）
/// - Credit 类型: "$X.XX left" 或 "$X.XX over"（负数）
/// - 其他类型: "X% left" 或 "X% over"（负数）
pub fn quota_remaining_text(quota: &QuotaInfo) -> String {
    if let Some(balance) = quota.remaining_balance {
        // 余额模式：直接显示余额
        if matches!(quota.quota_type, QuotaType::Credit) {
            format!("${:.2}", balance)
        } else {
            format!("{:.2}", balance)
        }
    } else {
        match quota.quota_type {
            QuotaType::Credit => {
                let remaining = quota.limit - quota.used;
                if remaining >= 0.0 {
                    t!("quota.credit_left", amount = format!("{:.2}", remaining)).to_string()
                } else {
                    t!("quota.credit_over", amount = format!("{:.2}", -remaining)).to_string()
                }
            }
            _ => {
                let pct = quota.percent_remaining();
                if pct >= 0.0 {
                    t!("quota.pct_left", pct = format!("{:.0}", pct)).to_string()
                } else {
                    t!("quota.pct_over", pct = format!("{:.0}", -pct)).to_string()
                }
            }
        }
    }
}

/// 使用详情文本（用于 UI 详细展示）
///
/// 从 `QuotaInfo` 的实例方法提取到 selector 层，
/// 消除数据模型对 i18n 的依赖（DIP 原则）。
///
/// - 余额模式: "Used: $X.XX" 或 空
/// - Credit 类型: "$X.XX / $Y.YY"
/// - 其他类型: "X used / Y total" 或 "X% used"
pub fn quota_usage_detail_text(quota: &QuotaInfo) -> String {
    if quota.remaining_balance.is_some() {
        // 余额模式：显示已用额度
        if quota.used > 0.0 {
            if matches!(quota.quota_type, QuotaType::Credit) {
                format!("Used: ${:.2}", quota.used)
            } else {
                format!("Used: {:.2}", quota.used)
            }
        } else {
            String::new()
        }
    } else {
        match quota.quota_type {
            QuotaType::Credit => t!(
                "quota.credit_detail",
                used = format!("{:.2}", quota.used),
                limit = format!("{:.2}", quota.limit)
            )
            .to_string(),
            _ => {
                if quota.is_percentage_mode() {
                    t!("quota.pct_used", pct = format!("{:.0}", quota.used)).to_string()
                } else {
                    t!(
                        "quota.count_detail",
                        used = format!("{:.0}", quota.used),
                        total = format!("{:.0}", quota.limit)
                    )
                    .to_string()
                }
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
    use crate::models::{ConnectionStatus, ProviderKind, QuotaInfo, QuotaType, UpdateStatus};

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

    // ── format_last_updated ──────────────────────────────────

    #[test]
    fn format_last_updated_no_instant_connected() {
        let _locale_guard = setup_locale();
        let p = make_provider(ProviderKind::Claude, ConnectionStatus::Connected);
        assert_eq!(format_last_updated(&p), "Waiting for data");
    }

    #[test]
    fn format_last_updated_no_instant_refreshing() {
        let _locale_guard = setup_locale();
        let p = make_provider(ProviderKind::Claude, ConnectionStatus::Refreshing);
        assert_eq!(format_last_updated(&p), "Refreshing…");
    }

    #[test]
    fn format_last_updated_no_instant_error() {
        let _locale_guard = setup_locale();
        let p = make_provider(ProviderKind::Claude, ConnectionStatus::Error);
        assert_eq!(format_last_updated(&p), "Needs attention");
    }

    #[test]
    fn format_last_updated_no_instant_disconnected() {
        let _locale_guard = setup_locale();
        let p = make_provider(ProviderKind::Claude, ConnectionStatus::Disconnected);
        assert_eq!(format_last_updated(&p), "Not connected");
    }

    #[test]
    fn format_last_updated_with_failed_status() {
        let _locale_guard = setup_locale();
        let mut p = make_provider(ProviderKind::Claude, ConnectionStatus::Connected);
        p.update_status = Some(UpdateStatus::Failed);
        assert_eq!(format_last_updated(&p), "Update failed");
    }

    #[test]
    fn format_last_updated_just_now() {
        let _locale_guard = setup_locale();
        let mut p = make_provider(ProviderKind::Claude, ConnectionStatus::Connected);
        p.last_refreshed_instant = Some(std::time::Instant::now());
        assert_eq!(format_last_updated(&p), "Updated just now");
    }

    #[test]
    fn format_last_updated_exactly_60s() {
        let _locale_guard = setup_locale();
        let mut p = make_provider(ProviderKind::Claude, ConnectionStatus::Connected);
        p.last_refreshed_instant =
            Some(std::time::Instant::now() - std::time::Duration::from_secs(60));
        assert_eq!(format_last_updated(&p), "Updated 1 min ago");
    }

    #[test]
    fn format_last_updated_exactly_3600s() {
        let _locale_guard = setup_locale();
        let mut p = make_provider(ProviderKind::Claude, ConnectionStatus::Connected);
        p.last_refreshed_instant =
            Some(std::time::Instant::now() - std::time::Duration::from_secs(3600));
        assert_eq!(format_last_updated(&p), "Updated 1 hr ago");
    }

    #[test]
    fn format_last_updated_instant_takes_priority_over_status() {
        let _locale_guard = setup_locale();
        let mut p = make_provider(ProviderKind::Claude, ConnectionStatus::Error);
        p.last_refreshed_instant = Some(std::time::Instant::now());
        p.update_status = Some(UpdateStatus::Failed);
        // instant 存在时，优先显示时间，不显示 "Update failed"
        assert_eq!(format_last_updated(&p), "Updated just now");
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

    // ── provider_list_subtitle ───────────────────────────────

    #[test]
    fn subtitle_disabled_shows_source() {
        let _locale_guard = setup_locale();
        let p = make_provider(ProviderKind::Claude, ConnectionStatus::Connected);
        let subtitle = provider_list_subtitle(&p, false);
        assert!(subtitle.contains("test"));
    }

    #[test]
    fn subtitle_connected_with_email() {
        let _locale_guard = setup_locale();
        let mut p = make_provider(ProviderKind::Claude, ConnectionStatus::Connected);
        p.account_email = Some("user@example.com".into());
        assert_eq!(provider_list_subtitle(&p, true), "user@example.com");
    }

    #[test]
    fn subtitle_error_with_message() {
        let _locale_guard = setup_locale();
        let mut p = make_provider(ProviderKind::Claude, ConnectionStatus::Error);
        p.error_message = Some("auth expired".into());
        let subtitle = provider_list_subtitle(&p, true);
        assert!(subtitle.contains("test")); // source_label
    }

    // ── quota_remaining_text ─────────────────────────────────

    #[test]
    fn remaining_text_percentage() {
        let _locale_guard = setup_locale();
        let q = QuotaInfo::new("test", 30.0, 100.0);
        assert_eq!(quota_remaining_text(&q), "70% left");

        let q_depleted = QuotaInfo::new("depleted", 100.0, 100.0);
        assert_eq!(quota_remaining_text(&q_depleted), "0% left");

        // 测试负数（超出配额）
        let q_over = QuotaInfo::new("over", 120.0, 100.0);
        assert_eq!(quota_remaining_text(&q_over), "20% over");
    }

    #[test]
    fn remaining_text_credit() {
        let _locale_guard = setup_locale();
        let q = QuotaInfo::with_details("Credit", 5.0, 20.0, QuotaType::Credit, None);
        assert_eq!(quota_remaining_text(&q), "$15.00 left");

        let q_zero = QuotaInfo::with_details("Credit", 20.0, 20.0, QuotaType::Credit, None);
        assert_eq!(quota_remaining_text(&q_zero), "$0.00 left");

        let q_exceeded = QuotaInfo::with_details("Credit", 25.0, 20.0, QuotaType::Credit, None);
        assert_eq!(quota_remaining_text(&q_exceeded), "$5.00 over");
    }

    #[test]
    fn remaining_text_balance_credit() {
        let _locale_guard = setup_locale();
        let q = QuotaInfo::balance_only("B", 15.50, None, QuotaType::Credit, None);
        assert_eq!(quota_remaining_text(&q), "$15.50");
    }

    #[test]
    fn remaining_text_balance_general() {
        let _locale_guard = setup_locale();
        let q = QuotaInfo::balance_only("B", 42.0, None, QuotaType::General, None);
        assert_eq!(quota_remaining_text(&q), "42.00");
    }

    // ── quota_usage_detail_text ──────────────────────────────

    #[test]
    fn usage_detail_text_percentage() {
        let _locale_guard = setup_locale();
        let q = QuotaInfo::new("test", 30.0, 100.0);
        assert_eq!(quota_usage_detail_text(&q), "30% used");

        let q_full = QuotaInfo::new("full", 100.0, 100.0);
        assert_eq!(quota_usage_detail_text(&q_full), "100% used");

        // 非 percentage mode（limit != 100）
        let q_real = QuotaInfo::new("real", 30.0, 50.0);
        assert_eq!(quota_usage_detail_text(&q_real), "30 used / 50 total");
    }

    #[test]
    fn usage_detail_text_credit() {
        let _locale_guard = setup_locale();
        let q = QuotaInfo::with_details("Credit", 5.0, 20.0, QuotaType::Credit, None);
        assert_eq!(quota_usage_detail_text(&q), "$5.00 / $20.00");

        let q_zero = QuotaInfo::with_details("Credit", 0.0, 100.0, QuotaType::Credit, None);
        assert_eq!(quota_usage_detail_text(&q_zero), "$0.00 / $100.00");
    }

    #[test]
    fn usage_detail_text_balance_with_used() {
        let _locale_guard = setup_locale();
        let q = QuotaInfo::balance_only("B", 10.0, Some(3.50), QuotaType::Credit, None);
        assert_eq!(quota_usage_detail_text(&q), "Used: $3.50");
    }

    #[test]
    fn usage_detail_text_balance_zero_used() {
        let _locale_guard = setup_locale();
        let q = QuotaInfo::balance_only("B", 10.0, None, QuotaType::Credit, None);
        assert_eq!(quota_usage_detail_text(&q), "");
    }
}
