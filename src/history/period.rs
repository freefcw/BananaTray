//! 配额当前周期的推断：只给托盘配额卡的「本周期」折线用。
//!
//! 周期终点来自最近一次采样的 `reset_at_secs`；起点优先按标签的固定长度倒推
//! （session 5h / daily 24h / weekly 7d / monthly 一个自然月），标签认不出时按
//! `quota_type` 兜底，再认不出就从同一重置时间的连续采样往前推，优先取能盖住证据的
//! 最小周期。

use super::sample::{HistoryStatus, HistoryValueKind};
use super::series::HistoryRange;
use super::store::{HistoryPointRow, HistoryRow};
use super::DAY_MS;
use crate::models::QuotaLabelSpec;

const HOUR_MS: i64 = 3_600_000;
const SESSION_MS: i64 = 5 * HOUR_MS;
const WEEK_MS: i64 = 7 * DAY_MS;

/// 有的上游用「现在 + 剩余秒数」算重置时间，同一周期内会抖几秒到几分钟；
/// 相邻两个周期至少差 5 小时，10 分钟以内仍算同一个重置。
const SAME_RESET_TOLERANCE_MS: i64 = 10 * 60 * 1000;

/// 配额当前周期。`end_ms` 是重置时间，`start_ms` 是周期起点。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuotaPeriod {
    pub start_ms: i64,
    pub end_ms: i64,
}

/// 周期的固定长度，或「一个自然月」这种跟着终点走的非定长。
#[derive(Debug, Clone, Copy)]
enum PeriodLength {
    Fixed(i64),
    CalendarMonth,
}

impl PeriodLength {
    fn start(self, end_ms: i64) -> Option<i64> {
        match self {
            Self::Fixed(ms) => Some(end_ms.saturating_sub(ms)),
            Self::CalendarMonth => calendar_month_before(end_ms),
        }
    }
}

fn calendar_month_before(end_ms: i64) -> Option<i64> {
    Some(
        chrono::DateTime::<chrono::Utc>::from_timestamp_millis(end_ms)?
            .checked_sub_months(chrono::Months::new(1))?
            .timestamp_millis(),
    )
}

/// 标签明确给出的周期长度。认不出周期语义的标签返回 `None`。
/// 新标签必须在这里显式归类，不写 `_` 兜底，漏归类会编译失败。
fn label_length(label_spec_json: &str) -> Option<PeriodLength> {
    let spec = serde_json::from_str::<QuotaLabelSpec>(label_spec_json).ok()?;
    match spec {
        QuotaLabelSpec::Session | QuotaLabelSpec::ModelSpecificSession { .. } => {
            Some(PeriodLength::Fixed(SESSION_MS))
        }
        QuotaLabelSpec::RollingHours { hours } => {
            (hours > 0).then(|| PeriodLength::Fixed(i64::from(hours) * HOUR_MS))
        }
        QuotaLabelSpec::Daily => Some(PeriodLength::Fixed(DAY_MS)),
        QuotaLabelSpec::Weekly
        | QuotaLabelSpec::WeeklyModel { .. }
        | QuotaLabelSpec::WeeklyTier { .. } => Some(PeriodLength::Fixed(WEEK_MS)),
        QuotaLabelSpec::Monthly
        | QuotaLabelSpec::MonthlyTier { .. }
        | QuotaLabelSpec::MonthlyCredits => Some(PeriodLength::CalendarMonth),
        // 这些标签看不出周期长度，交给 quota_type 或采样证据推断。
        QuotaLabelSpec::Raw(_)
        | QuotaLabelSpec::Credits
        | QuotaLabelSpec::BonusCredits
        | QuotaLabelSpec::CloudCredits
        | QuotaLabelSpec::LimitResetCredits
        | QuotaLabelSpec::ExtraUsage
        | QuotaLabelSpec::PremiumRequests { .. }
        | QuotaLabelSpec::ChatCompletions { .. }
        | QuotaLabelSpec::OnDemand
        | QuotaLabelSpec::Team
        | QuotaLabelSpec::SubscriptionUsage { .. } => None,
    }
}

/// 标签认不出时按 `quota_type` 兜底。`model:*` 不映射——MiniMax 把非周配额
/// 也标成 ModelSpecific，只能靠证据推。
fn quota_type_length(quota_type: &str) -> Option<PeriodLength> {
    match quota_type {
        "session" => Some(PeriodLength::Fixed(SESSION_MS)),
        "weekly" => Some(PeriodLength::Fixed(WEEK_MS)),
        "monthly" => Some(PeriodLength::CalendarMonth),
        _ => None,
    }
}

/// 当前周期的范围。认不出周期（没有重置时间、周期已结束、证据推不出起点）返回 `None`。
pub(super) fn current_period(
    rows: &[HistoryRow],
    quota_key: &str,
    now_ms: i64,
) -> Option<QuotaPeriod> {
    // 每行取这个 key 的最后一点，与 series_for_key 同规则。按时间倒序，最新在前。
    let mut samples: Vec<(i64, &HistoryPointRow)> = Vec::new();
    for row in rows {
        if row.status != HistoryStatus::Success {
            continue;
        }
        if let Some(point) = row
            .points
            .iter()
            .rev()
            .find(|point| point.quota_key == quota_key)
        {
            samples.push((row.captured_at_ms, point));
        }
    }
    samples.sort_by_key(|(captured, _)| std::cmp::Reverse(*captured));

    let (_, latest) = samples.first()?;
    if latest.value_kind != HistoryValueKind::Metered {
        return None;
    }
    latest.limit_value?;
    let end_ms = latest.reset_at_secs?.saturating_mul(1000);
    if end_ms <= now_ms {
        return None;
    }

    let length =
        label_length(&latest.label_spec_json).or_else(|| quota_type_length(&latest.quota_type));
    if let Some(length) = length {
        let start_ms = length.start(end_ms)?;
        if start_ms > now_ms {
            return None;
        }
        return Some(QuotaPeriod { start_ms, end_ms });
    }

    infer_period(&samples, end_ms)
}

/// 长度未知时从证据推：最新一段重置时间相同的连续采样（±10 分钟算同一个重置），
/// 最早那条采样的时刻是证据起点。候选按从小到大排，第一个起点早于证据的赢。
fn infer_period(samples: &[(i64, &HistoryPointRow)], end_ms: i64) -> Option<QuotaPeriod> {
    let mut evidence_start_ms: Option<i64> = None;
    for (captured, point) in samples {
        let same_reset = point.value_kind == HistoryValueKind::Metered
            && point.reset_at_secs.is_some_and(|reset| {
                (reset.saturating_mul(1000) - end_ms).abs() <= SAME_RESET_TOLERANCE_MS
            });
        if !same_reset {
            break;
        }
        evidence_start_ms = Some(*captured);
    }
    let evidence_start_ms = evidence_start_ms?;
    for length in [
        PeriodLength::Fixed(SESSION_MS),
        PeriodLength::Fixed(DAY_MS),
        PeriodLength::Fixed(WEEK_MS),
        PeriodLength::CalendarMonth,
    ] {
        let start_ms = length.start(end_ms)?;
        if start_ms <= evidence_start_ms {
            return Some(QuotaPeriod { start_ms, end_ms });
        }
    }
    None
}

/// 与设置页同长度范围的桶宽一致。
pub(super) fn period_bucket_width_ms(period: QuotaPeriod) -> i64 {
    let span = period.end_ms.saturating_sub(period.start_ms);
    if span <= DAY_MS {
        HistoryRange::Last24Hours.bucket_width_ms()
    } else if span <= WEEK_MS {
        HistoryRange::Last7Days.bucket_width_ms()
    } else {
        HistoryRange::Last30Days.bucket_width_ms()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::sample::HistoryValueKind;
    use crate::history::{HistoryPointRow, HistoryRow, HistoryStatus, HistoryUnit};

    const NOW: i64 = 500 * DAY_MS;

    fn point(
        key: &str,
        quota_type: &str,
        label_spec_json: &str,
        value_kind: HistoryValueKind,
        reset_at_secs: Option<i64>,
    ) -> HistoryPointRow {
        HistoryPointRow {
            quota_key: key.to_string(),
            quota_type: quota_type.to_string(),
            label_spec_json: label_spec_json.to_string(),
            value_kind,
            unit: Some(HistoryUnit::Percentage),
            used: Some(10.0),
            remaining: match value_kind {
                HistoryValueKind::Balance => Some(60.0),
                _ => None,
            },
            limit_value: Some(100.0),
            reset_at_secs,
        }
    }

    fn metered(key: &str, quota_type: &str, label: &str, reset_at_ms: i64) -> HistoryPointRow {
        point(
            key,
            quota_type,
            label,
            HistoryValueKind::Metered,
            Some(reset_at_ms / 1000),
        )
    }

    fn row(id: i64, captured: i64, points: Vec<HistoryPointRow>) -> HistoryRow {
        HistoryRow {
            sample_id: id,
            captured_at_ms: captured,
            provider_id: "codex".to_string(),
            status: HistoryStatus::Success,
            error_kind: None,
            failure_reason: None,
            failure_reason_payload: None,
            failure_advice_json: None,
            detail: None,
            refresh_reason: None,
            points,
        }
    }

    #[test]
    fn weekly_label_starts_seven_days_before_reset() {
        let end = NOW + 3 * DAY_MS;
        let rows = [row(
            1,
            NOW,
            vec![metered("weekly", "general", "\"Weekly\"", end)],
        )];
        let period = current_period(&rows, "weekly", NOW).expect("period");
        assert_eq!(period.end_ms, end);
        assert_eq!(period.start_ms, end - 7 * DAY_MS);
    }

    #[test]
    fn session_label_is_five_hours() {
        let end = NOW + 2 * HOUR_MS;
        let rows = [row(
            1,
            NOW,
            vec![metered("session", "general", "\"Session\"", end)],
        )];
        let period = current_period(&rows, "session", NOW).expect("period");
        assert_eq!(period.start_ms, end - SESSION_MS);
    }

    #[test]
    fn unknown_label_infers_the_smallest_covering_period() {
        let end = NOW + 3 * HOUR_MS;
        let label = "{\"Raw\":\"Pro\"}";
        // 一条刚采的样本：5 小时周期的起点就盖住它。
        let rows = [row(1, NOW, vec![metered("pro", "general", label, end)])];
        let period = current_period(&rows, "pro", NOW).expect("period");
        assert_eq!(period.start_ms, end - SESSION_MS);

        // 同一个重置时间的样本能追到 10 小时前：5 小时盖不住，24 小时能。
        let rows = [
            row(
                1,
                NOW - 10 * HOUR_MS,
                vec![metered("pro", "general", label, end)],
            ),
            row(2, NOW, vec![metered("pro", "general", label, end)]),
        ];
        let period = current_period(&rows, "pro", NOW).expect("period");
        assert_eq!(period.start_ms, end - DAY_MS);

        // 追到 3 天前：只有 7 天能盖住。
        let rows = [
            row(
                1,
                NOW - 3 * DAY_MS,
                vec![metered("pro", "general", label, end)],
            ),
            row(2, NOW, vec![metered("pro", "general", label, end)]),
        ];
        let period = current_period(&rows, "pro", NOW).expect("period");
        assert_eq!(period.start_ms, end - WEEK_MS);
    }

    #[test]
    fn a_sample_from_the_previous_cycle_is_not_evidence() {
        let end = NOW + 3 * HOUR_MS;
        let label = "{\"Raw\":\"Pro\"}";
        let rows = [
            // 上一个周期的样本：重置时间不同，截断证据链。
            row(
                1,
                NOW - 10 * HOUR_MS,
                vec![metered("pro", "general", label, end + 5 * DAY_MS)],
            ),
            row(2, NOW, vec![metered("pro", "general", label, end)]),
        ];
        let period = current_period(&rows, "pro", NOW).expect("period");
        assert_eq!(period.start_ms, end - SESSION_MS);
    }

    #[test]
    fn model_quota_type_does_not_map_to_weekly() {
        let end = NOW + 3 * HOUR_MS;
        let label = "{\"Raw\":\"Pro\"}";
        // MiniMax 把非周配额也标成 ModelSpecific：走证据推断，不当 7 天。
        let rows = [
            row(
                1,
                NOW - 10 * HOUR_MS,
                vec![metered("pro", "model:minimax", label, end)],
            ),
            row(2, NOW, vec![metered("pro", "model:minimax", label, end)]),
        ];
        let period = current_period(&rows, "pro", NOW).expect("period");
        assert_eq!(period.start_ms, end - DAY_MS);
    }

    #[test]
    fn unresolvable_periods_return_none() {
        // 重置时间已过。
        let rows = [row(
            1,
            NOW,
            vec![metered("weekly", "general", "\"Weekly\"", NOW)],
        )];
        assert_eq!(current_period(&rows, "weekly", NOW), None);

        // 余额型配额没有周期。
        let rows = [row(
            1,
            NOW,
            vec![point(
                "credit",
                "general",
                "\"Credits\"",
                HistoryValueKind::Balance,
                Some(NOW / 1000 + 3600),
            )],
        )];
        assert_eq!(current_period(&rows, "credit", NOW), None);

        // 没有重置时间。
        let rows = [row(
            1,
            NOW,
            vec![point(
                "weekly",
                "general",
                "\"Weekly\"",
                HistoryValueKind::Metered,
                None,
            )],
        )];
        assert_eq!(current_period(&rows, "weekly", NOW), None);
    }

    #[test]
    fn monthly_label_subtracts_one_calendar_month() {
        let end = chrono::NaiveDate::from_ymd_opt(2026, 11, 15)
            .unwrap()
            .and_hms_opt(8, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp_millis();
        let now = end - 10 * DAY_MS;
        let rows = [row(
            1,
            now,
            vec![metered("monthly", "general", "\"Monthly\"", end)],
        )];
        let period = current_period(&rows, "monthly", now).expect("period");
        let expected = chrono::NaiveDate::from_ymd_opt(2026, 10, 15)
            .unwrap()
            .and_hms_opt(8, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp_millis();
        assert_eq!(period.start_ms, expected);
    }

    #[test]
    fn quota_type_fills_in_when_the_label_is_raw() {
        let label = "{\"Raw\":\"MONTH (Gemini)\"}";
        let end = chrono::NaiveDate::from_ymd_opt(2026, 11, 15)
            .unwrap()
            .and_hms_opt(8, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp_millis();
        let now = end - 10 * DAY_MS;
        // Antigravity 的 Raw 标签配 Monthly 类型：起点是重置前的一个自然月。
        let rows = [row(1, now, vec![metered("quota", "monthly", label, end)])];
        let period = current_period(&rows, "quota", now).expect("period");
        let expected = chrono::NaiveDate::from_ymd_opt(2026, 10, 15)
            .unwrap()
            .and_hms_opt(8, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp_millis();
        assert_eq!(period.start_ms, expected);

        let end = NOW + 3 * DAY_MS;
        let rows = [row(1, NOW, vec![metered("quota", "weekly", label, end)])];
        let period = current_period(&rows, "quota", NOW).expect("period");
        assert_eq!(period.start_ms, end - WEEK_MS);

        let end = NOW + 3 * HOUR_MS;
        let rows = [row(1, NOW, vec![metered("quota", "session", label, end)])];
        let period = current_period(&rows, "quota", NOW).expect("period");
        assert_eq!(period.start_ms, end - SESSION_MS);
    }

    #[test]
    fn daily_and_rolling_hours_lengths() {
        let end = NOW + 6 * HOUR_MS;
        let rows = [row(
            1,
            NOW,
            vec![metered("daily", "general", "\"Daily\"", end)],
        )];
        let period = current_period(&rows, "daily", NOW).expect("period");
        assert_eq!(period.start_ms, end - DAY_MS);

        let rows = [row(
            1,
            NOW,
            vec![metered(
                "rolling-10h",
                "general",
                "{\"RollingHours\":{\"hours\":10}}",
                end,
            )],
        )];
        let period = current_period(&rows, "rolling-10h", NOW).expect("period");
        assert_eq!(period.start_ms, end - 10 * HOUR_MS);
    }

    #[test]
    fn zero_rolling_hours_falls_through_to_inference() {
        let end = NOW + 3 * HOUR_MS;
        // hours: 0 不是合法滚动窗口，按证据推：一条新样本 → 最小的 5 小时。
        let rows = [row(
            1,
            NOW,
            vec![metered(
                "rolling-0h",
                "general",
                "{\"RollingHours\":{\"hours\":0}}",
                end,
            )],
        )];
        let period = current_period(&rows, "rolling-0h", NOW).expect("period");
        assert_eq!(period.start_ms, end - SESSION_MS);
    }
}
