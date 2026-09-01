//! 现行订阅行（Amp CLI `0.0.1788192028` / 2026-08-31 起）：
//!
//! ```text
//! **Amp <Plan> Subscription:** agent usage $6.42 of $20 remaining (32%), orb usage 750h of 750h a1.small orb hours remaining (100%) - period 2026-08-30 to 2026-09-30, ends in 29 days
//! ```
//!
//! 特点（相对 2026-08-17 过渡格式）：
//! - label 外包了一层 markdown 加粗 `**...**`（`--no-color` 也剥不掉），
//!   统一在上游 `mod.rs::strip_markdown_bold` 剥掉，本策略只见到裸文本。
//! - 池片段从纯百分比（`X% other usage`）改为绝对值 + 括号百分比：
//!   `agent usage $6.42 of $20 remaining (32%)`（agent 调用额度，美元）、
//!   `orb usage 750h of 750h a1.small orb hours remaining (100%)`（远程实例，小时）。
//!   上游同时把池名 `other` 改成了 `agent`。
//! - 行尾追加 `- period <起> to <止>, ends in N days`，暂不入模型。
//!
//! 进度条仍用百分比模式（CLI 括号里的百分比），绝对值原文透传到详情行，
//! 避免美元池与小时池混用 `QuotaType::Credit` 的单位问题。

use super::{SubscriptionLineStrategy, VALUE_POOL_RE};
use crate::models::{QuotaDetailSpec, QuotaInfo, QuotaLabelSpec, QuotaType};
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
        let plan = caps[1].trim();
        let rest = &caps[2];

        let quotas: Vec<QuotaInfo> = VALUE_POOL_RE
            .captures_iter(rest)
            .map(|pool_caps| {
                // pool 保留 CLI 原文小写（agent / orb），selector 再按 locale 渲染
                let pool = pool_caps[1].to_ascii_lowercase();
                let remaining_token = &pool_caps[2];
                let total_token = &pool_caps[3];
                // 优先用 CLI 给的百分比；缺失时由绝对值换算
                let remaining_percent = pool_caps
                    .get(4)
                    .and_then(|m| m.as_str().parse::<f64>().ok())
                    .or_else(|| {
                        let remaining = parse_value_token(remaining_token)?;
                        let total = parse_value_token(total_token)?;
                        if total > 0.0 {
                            Some(remaining / total * 100.0)
                        } else {
                            None
                        }
                    })
                    .unwrap_or(0.0);
                QuotaInfo::from_remaining_percent(
                    QuotaLabelSpec::SubscriptionUsage {
                        plan: plan.to_string(),
                        pool,
                    },
                    remaining_percent,
                    QuotaType::General,
                    Some(QuotaDetailSpec::Raw(format!(
                        "{remaining_token} of {total_token}"
                    ))),
                )
            })
            .collect();

        if quotas.is_empty() {
            return None;
        }
        Some(quotas)
    }
}

/// 解析绝对值 token：`$6.42` → 6.42，`750h` → 750。认不出返回 None。
fn parse_value_token(token: &str) -> Option<f64> {
    let t = token.trim_start_matches('$');
    let num_end = t
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(t.len());
    t[..num_end].parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::QuotaType;

    #[test]
    fn parses_current_value_based_line() {
        let quotas = CurrentSubscriptionLine::parse_line(
            "Amp Megawatt Subscription: agent usage $6.42 of $20 remaining (32%), orb usage 750h of 750h a1.small orb hours remaining (100%) - period 2026-08-30 to 2026-09-30, ends in 29 days",
        )
        .unwrap();

        assert_eq!(quotas.len(), 2);
        assert_eq!(quotas[0].stable_key, "subscription:megawatt:agent");
        // 32% remaining → used=68
        assert!((quotas[0].used - 68.0).abs() < f64::EPSILON);
        assert_eq!(quotas[0].limit, 100.0);
        assert_eq!(quotas[0].quota_type, QuotaType::General);
        assert_eq!(
            quotas[0].detail_spec,
            Some(QuotaDetailSpec::Raw("$6.42 of $20".to_string()))
        );

        assert_eq!(quotas[1].stable_key, "subscription:megawatt:orb");
        assert!((quotas[1].used - 0.0).abs() < f64::EPSILON);
        assert_eq!(
            quotas[1].detail_spec,
            Some(QuotaDetailSpec::Raw("750h of 750h".to_string()))
        );
    }

    /// 括号百分比缺失时由绝对值换算：$5 of $20 → 25% remaining
    #[test]
    fn falls_back_to_computed_percent() {
        let quotas = CurrentSubscriptionLine::parse_line(
            "Amp Megawatt Subscription: agent usage $5 of $20 remaining",
        )
        .unwrap();

        assert_eq!(quotas.len(), 1);
        assert!((quotas[0].used - 75.0).abs() < f64::EPSILON);
        assert_eq!(quotas[0].limit, 100.0);
    }

    /// 百分比形态（2026-08-17 过渡格式）的行不属于本策略
    #[test]
    fn ignores_percent_pool_line() {
        assert!(CurrentSubscriptionLine::parse_line(
            "Amp Megawatt Subscription: 75% other usage and 100% orb usage remaining"
        )
        .is_none());
    }
}
