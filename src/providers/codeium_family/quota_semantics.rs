use crate::models::{QuotaDetailSpec, QuotaInfo, QuotaLabelSpec, QuotaType};

/// Devin Desktop 省略 active weekly percentage 时，将其解释为本周期已耗尽。
///
/// reset 已过期或缺失时无法区分“耗尽”与“陈旧快照”，因此不生成配额。
pub(crate) fn infer_exhausted_weekly_quota(
    remaining_percent: Option<f64>,
    reset_at_unix: Option<i64>,
    now_unix: i64,
) -> Option<QuotaInfo> {
    if remaining_percent.is_some() {
        return None;
    }
    let reset_at = reset_at_unix.filter(|reset_at| *reset_at > now_unix)?;

    Some(QuotaInfo::with_key_from_remaining_percent(
        "weekly-quota",
        QuotaLabelSpec::Weekly,
        0.0,
        QuotaType::Weekly,
        Some(QuotaDetailSpec::ResetAt {
            epoch_secs: reset_at,
        }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn future_reset_infers_exhausted_weekly_quota() {
        let quota = infer_exhausted_weekly_quota(None, Some(2_000), 1_000).unwrap();

        assert_eq!(quota.stable_key, "weekly-quota");
        assert_eq!(quota.quota_type, QuotaType::Weekly);
        assert_eq!(quota.used, 100.0);
        assert_eq!(quota.limit, 100.0);
        assert_eq!(
            quota.detail_spec,
            Some(QuotaDetailSpec::ResetAt { epoch_secs: 2_000 })
        );
    }

    #[test]
    fn present_percentage_or_inactive_reset_does_not_infer_quota() {
        assert!(infer_exhausted_weekly_quota(Some(0.0), Some(2_000), 1_000).is_none());
        assert!(infer_exhausted_weekly_quota(None, None, 1_000).is_none());
        assert!(infer_exhausted_weekly_quota(None, Some(1_000), 1_000).is_none());
        assert!(infer_exhausted_weekly_quota(None, Some(999), 1_000).is_none());
    }
}
