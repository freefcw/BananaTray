/// 百分比制配额的完整尺度。
///
/// 仅供 `QuotaInfo` 百分比构造器内部使用，避免 provider 重复知道
/// 百分比模式的 `limit` 实现细节。
pub(super) const PERCENT_SCALE: f64 = 100.0;

/// fraction 制配额的完整剩余额度。
pub(super) const FULL_REMAINING_FRACTION: f64 = 1.0;

pub(super) fn used_percent_from_remaining_percent(remaining_percent: f64) -> f64 {
    PERCENT_SCALE - remaining_percent
}

pub(super) fn used_percent_from_remaining_fraction(remaining_fraction: f64) -> f64 {
    used_percent_from_remaining_percent(remaining_fraction * PERCENT_SCALE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn used_percent_from_remaining_percent_preserves_raw_percentage_math() {
        assert_eq!(used_percent_from_remaining_percent(PERCENT_SCALE), 0.0);
        assert_eq!(used_percent_from_remaining_percent(40.0), 60.0);
        assert_eq!(used_percent_from_remaining_percent(0.0), PERCENT_SCALE);
    }

    #[test]
    fn used_percent_from_remaining_fraction_converts_fraction_to_percentage() {
        assert_eq!(
            used_percent_from_remaining_fraction(FULL_REMAINING_FRACTION),
            0.0
        );
        assert_eq!(used_percent_from_remaining_fraction(0.25), 75.0);
        assert_eq!(used_percent_from_remaining_fraction(0.0), PERCENT_SCALE);
    }
}
