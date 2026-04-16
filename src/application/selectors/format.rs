//! 格式化与展示文案函数
//!
//! 将 Provider 状态/Quota → 展示文本 的转换逻辑集中于此。
//! 上次更新时间、配额使用详情。
//! 从原 `app/provider_logic.rs` 合并而来。

use crate::models::{ConnectionStatus, ProviderStatus, QuotaInfo, QuotaType, UpdateStatus};
use rust_i18n::t;

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
