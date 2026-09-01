//! 旧版订阅行：`Subscription <Plan>: X% other usage and Y% orb usage remaining`
//!
//! # 生命周期
//!
//! - **失效**：Amp CLI `0.0.1786939945`（发布于 2026-08-17）起，官方 `amp usage`
//!   输出改为 `Amp <Plan> Subscription: ...`，当前 CLI 不再产生本格式。
//! - **仍保留**：该版本之前的 CLI 仍输出本行。
//! - **预计废弃**：2026-11-17（新格式发布满 3 个月）。删除前先搜日志
//!   `amp: subscription matched legacy`，确认没有旧 CLI 命中后再删本文件，
//!   并从 `subscription/mod.rs` 的策略列表去掉 `LegacySubscriptionLine`。

use super::{quotas_from_pool_text, SubscriptionLineStrategy};
use crate::models::QuotaInfo;
use regex::Regex;
use std::sync::LazyLock;

/// 官方输出改为现行格式的日期（Amp CLI `0.0.1786939945`）。
pub(super) const OBSOLETE_SINCE: &str = "2026-08-17";
/// 计划删除本策略的最早日期。到期后先看 debug 日志再删，不是自动失效。
pub(super) const RETIRE_AFTER: &str = "2026-11-17";

static LINE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^Subscription\s+(.+?):\s*(.+)$").unwrap());

pub(super) struct LegacySubscriptionLine;

impl SubscriptionLineStrategy for LegacySubscriptionLine {
    fn name() -> &'static str {
        "legacy"
    }

    fn parse_line(line: &str) -> Option<Vec<QuotaInfo>> {
        let caps = LINE_RE.captures(line)?;
        let quotas = quotas_from_pool_text(caps[1].trim(), &caps[2]);
        // 认不出任何池片段时返回 None，让调度继续尝试后续策略
        if quotas.is_empty() {
            return None;
        }
        Some(quotas)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::QuotaLabelSpec;

    #[test]
    fn parses_subscription_plan_line() {
        let quotas = LegacySubscriptionLine::parse_line(
            "Subscription Megawatt: 81% other usage and 100% orb usage remaining",
        )
        .unwrap();

        assert_eq!(quotas.len(), 2);
        assert_eq!(
            quotas[0].label_spec,
            QuotaLabelSpec::SubscriptionUsage {
                plan: "Megawatt".into(),
                pool: "other".into(),
            }
        );
        assert!((quotas[0].used - 19.0).abs() < f64::EPSILON);
        assert_eq!(quotas[1].stable_key, "subscription:megawatt:orb");
    }

    #[test]
    fn ignores_current_amp_plan_subscription_prefix() {
        assert!(LegacySubscriptionLine::parse_line(
            "Amp Megawatt Subscription: 75% other usage and 100% orb usage remaining"
        )
        .is_none());
    }
}
