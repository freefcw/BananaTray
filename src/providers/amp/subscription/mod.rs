//! 订阅行解析：各代文案各一套策略，互不混写。
//!
//! 调度顺序：现行策略先认领，认不出再按代际从新到旧交给后续策略。
//! 删除过时版本时去掉对应 `interim.rs` / `legacy.rs` 文件以及本文件里
//! 对应的 `if`。

mod current;
mod interim;
mod legacy;

use crate::models::{QuotaInfo, QuotaLabelSpec, QuotaType};
use regex::Regex;
use std::sync::LazyLock;

/// 池片段通用匹配（过渡 / 旧版策略共用）：`81% other usage` / `100% orb usage`。
static SUBSCRIPTION_POOL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([0-9]+(?:\.[0-9]+)?)%\s+(\w+)\s+usage").unwrap());

/// 现行池片段匹配（2026-08-31 起）：`agent usage $6.42 of $20 remaining (32%)` /
/// `orb usage 750h of 750h a1.small orb hours remaining (100%)`。
///
/// 捕获组：1=池名，2=剩余绝对值 token（`$6.42` / `750h`），3=总量 token，
/// 4=CLI 括号内百分比（可能缺失，由绝对值换算兜底）。
/// 描述词里出现逗号 / 括号时 `.*?` 不会贪婪跨段。
static VALUE_POOL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)([a-z][a-z0-9]*)\s+usage\s+(\S+)\s+of\s+(\S+).*?remaining(?:\s*\(([0-9]+(?:\.[0-9]+)?)%\))?",
    )
    .unwrap()
});

/// 订阅行策略。`None` 表示本策略不认领该行，让下一套继续试。
pub(super) trait SubscriptionLineStrategy {
    fn name() -> &'static str;
    fn parse_line(line: &str) -> Option<Vec<QuotaInfo>>;
}

pub(super) fn parse_line(line: &str) -> Option<Vec<QuotaInfo>> {
    if let Some(quotas) = current::CurrentSubscriptionLine::parse_line(line) {
        log::debug!(
            target: "providers",
            "amp: subscription matched {}",
            current::CurrentSubscriptionLine::name()
        );
        return Some(quotas);
    }
    // 裁汰过渡版：删除下面这个 if 以及 `interim.rs`。
    if let Some(quotas) = interim::InterimSubscriptionLine::parse_line(line) {
        log::debug!(
            target: "providers",
            "amp: subscription matched {} (obsolete since {}, retire after {})",
            interim::InterimSubscriptionLine::name(),
            interim::OBSOLETE_SINCE,
            interim::RETIRE_AFTER
        );
        return Some(quotas);
    }
    // 裁汰旧版：删除下面这个 if 以及 `legacy.rs`。
    if let Some(quotas) = legacy::LegacySubscriptionLine::parse_line(line) {
        log::debug!(
            target: "providers",
            "amp: subscription matched {} (obsolete since {}, retire after {})",
            legacy::LegacySubscriptionLine::name(),
            legacy::OBSOLETE_SINCE,
            legacy::RETIRE_AFTER
        );
        return Some(quotas);
    }
    None
}

/// 从已拆出的 plan + 池文本组装 quota（过渡 / 旧版百分比形态共用）。
/// 前缀匹配由各策略自己做。
pub(super) fn quotas_from_pool_text(plan: &str, rest: &str) -> Vec<QuotaInfo> {
    SUBSCRIPTION_POOL_RE
        .captures_iter(rest)
        .map(|pool_caps| {
            let remaining_percent: f64 = pool_caps[1].parse().unwrap_or(0.0);
            // pool 保留 CLI 原文小写（other / orb），selector 再按 locale 渲染
            let pool = pool_caps[2].to_ascii_lowercase();
            QuotaInfo::from_remaining_percent(
                QuotaLabelSpec::SubscriptionUsage {
                    plan: plan.to_string(),
                    pool,
                },
                remaining_percent,
                QuotaType::General,
                None,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strategies_do_not_claim_each_others_lines() {
        let current_line = "Amp Megawatt Subscription: agent usage $6.42 of $20 remaining (32%), orb usage 750h of 750h a1.small orb hours remaining (100%)";
        let interim_line =
            "Amp Megawatt Subscription: 75% other usage and 100% orb usage remaining";
        let legacy_line = "Subscription Megawatt: 81% other usage and 100% orb usage remaining";

        assert!(
            current::CurrentSubscriptionLine::parse_line(interim_line).is_none(),
            "current strategy must not claim the percent-pool line"
        );
        assert!(
            current::CurrentSubscriptionLine::parse_line(legacy_line).is_none(),
            "current strategy must not claim the obsolete Subscription <Plan>: line"
        );
        assert!(
            interim::InterimSubscriptionLine::parse_line(current_line).is_none(),
            "interim strategy must not claim the current value-based line"
        );
        assert!(
            interim::InterimSubscriptionLine::parse_line(legacy_line).is_none(),
            "interim strategy must not claim the obsolete Subscription <Plan>: line"
        );
        assert!(
            legacy::LegacySubscriptionLine::parse_line(current_line).is_none(),
            "legacy strategy must not claim the current value-based line"
        );
        assert!(
            legacy::LegacySubscriptionLine::parse_line(interim_line).is_none(),
            "legacy strategy must not claim the interim Amp <Plan> Subscription: line"
        );

        assert_eq!(
            current::CurrentSubscriptionLine::parse_line(current_line)
                .expect("current line")
                .len(),
            2
        );
        assert_eq!(
            interim::InterimSubscriptionLine::parse_line(interim_line)
                .expect("interim line")
                .len(),
            2
        );
        assert_eq!(
            legacy::LegacySubscriptionLine::parse_line(legacy_line)
                .expect("legacy line")
                .len(),
            2
        );
    }

    #[test]
    fn dispatcher_prefers_current_then_interim_then_legacy() {
        let current = parse_line(
            "Amp Gigawatt Subscription: agent usage $0.42 of $20 remaining (2%), orb usage 750h of 750h a1.small orb hours remaining (100%)",
        )
        .unwrap();
        assert_eq!(current[0].stable_key, "subscription:gigawatt:agent");
        assert_eq!(current[1].stable_key, "subscription:gigawatt:orb");

        let interim =
            parse_line("Amp Gigawatt Subscription: 38% other usage and 5% orb usage remaining")
                .unwrap();
        assert_eq!(interim[0].stable_key, "subscription:gigawatt:other");
        assert_eq!(interim[1].stable_key, "subscription:gigawatt:orb");

        let legacy =
            parse_line("Subscription Gigawatt: 38% other usage and 5% orb usage remaining")
                .unwrap();
        assert_eq!(legacy[0].stable_key, "subscription:gigawatt:other");
        assert_eq!(legacy[1].stable_key, "subscription:gigawatt:orb");

        assert!(parse_line("Amp Free: 75% remaining today (resets daily)").is_none());
    }
}
