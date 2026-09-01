//! 过渡订阅行：`Amp <Plan> Subscription: X% other usage and Y% orb usage remaining`
//!
//! # 生命周期
//!
//! - **生效**：Amp CLI `0.0.1786939945`（2026-08-17）起。
//! - **失效**：Amp CLI `0.0.1788192028`（2026-08-31）起，官方 `amp usage` 输出改为
//!   绝对值 + 括号百分比（见 `current.rs`），当前 CLI 不再产生本格式。
//! - **仍保留**：2026-08-17 ~ 2026-08-31 之间的 CLI 仍输出本行。
//! - **预计废弃**：2026-11-30（新格式发布满 3 个月）。删除前先搜日志
//!   `amp: subscription matched interim`，确认没有旧 CLI 命中后再删本文件，
//!   并从 `subscription/mod.rs` 的策略列表去掉 `InterimSubscriptionLine`。

use super::{quotas_from_pool_text, SubscriptionLineStrategy};
use crate::models::QuotaInfo;
use regex::Regex;
use std::sync::LazyLock;

/// 官方输出改为现行格式的日期（Amp CLI `0.0.1788192028`）。
pub(super) const OBSOLETE_SINCE: &str = "2026-08-31";
/// 计划删除本策略的最早日期。到期后先看 debug 日志再删，不是自动失效。
pub(super) const RETIRE_AFTER: &str = "2026-11-30";

static LINE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^Amp\s+(.+?)\s+Subscription:\s*(.+)$").unwrap());

pub(super) struct InterimSubscriptionLine;

impl SubscriptionLineStrategy for InterimSubscriptionLine {
    fn name() -> &'static str {
        "interim"
    }

    fn parse_line(line: &str) -> Option<Vec<QuotaInfo>> {
        let caps = LINE_RE.captures(line)?;
        let quotas = quotas_from_pool_text(caps[1].trim(), &caps[2]);
        // 认不出任何池片段时返回 None，让调度继续尝试后续策略
        // （现行格式的行前缀与本策略相同，只靠池片段区分）。
        if quotas.is_empty() {
            return None;
        }
        Some(quotas)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{QuotaLabelSpec, QuotaType};

    #[test]
    fn parses_amp_plan_subscription_line() {
        let quotas = InterimSubscriptionLine::parse_line(
            "Amp Megawatt Subscription: 75% other usage and 100% orb usage remaining",
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
        assert!((quotas[0].used - 25.0).abs() < f64::EPSILON);
        assert_eq!(quotas[0].quota_type, QuotaType::General);
        assert_eq!(quotas[1].stable_key, "subscription:megawatt:orb");
    }

    #[test]
    fn ignores_legacy_subscription_prefix() {
        assert!(InterimSubscriptionLine::parse_line(
            "Subscription Megawatt: 81% other usage and 100% orb usage remaining"
        )
        .is_none());
    }

    /// 行前缀与现行格式相同但池片段是绝对值形态：本策略必须放行（返回 None），
    /// 否则会截住现行策略的调度。
    #[test]
    fn does_not_claim_current_value_based_line() {
        assert!(InterimSubscriptionLine::parse_line(
            "Amp Megawatt Subscription: agent usage $6.42 of $20 remaining (32%), orb usage 750h of 750h a1.small orb hours remaining (100%)"
        )
        .is_none());
    }
}
