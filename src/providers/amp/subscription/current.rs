//! 现行订阅行：`Amp <Plan> Subscription: X% other usage and Y% orb usage remaining`
//!
//! 生效：Amp CLI `0.0.1786939945`（2026-08-17）及之后。

use super::{quotas_from_pool_text, SubscriptionLineStrategy};
use crate::models::QuotaInfo;
use regex::Regex;
use std::sync::LazyLock;

static LINE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^Amp\s+(.+?)\s+Subscription:\s*(.+)$").unwrap());

pub(super) struct CurrentSubscriptionLine;

impl SubscriptionLineStrategy for CurrentSubscriptionLine {
    fn name() -> &'static str {
        "current"
    }

    fn parse_line(line: &str) -> Option<Vec<QuotaInfo>> {
        let caps = LINE_RE.captures(line)?;
        Some(quotas_from_pool_text(caps[1].trim(), &caps[2]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{QuotaLabelSpec, QuotaType};

    #[test]
    fn parses_amp_plan_subscription_line() {
        let quotas = CurrentSubscriptionLine::parse_line(
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
        assert!(CurrentSubscriptionLine::parse_line(
            "Subscription Megawatt: 81% other usage and 100% orb usage remaining"
        )
        .is_none());
    }
}
