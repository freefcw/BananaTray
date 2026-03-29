// ============================================================================
// 弹出窗口布局常量与计算
// ============================================================================

/// 弹出窗口布局相关常量，集中管理避免 magic numbers
pub struct PopupLayout;

impl PopupLayout {
    /// 弹出窗口固定宽度（px）
    pub const WIDTH: f32 = 308.0;
    /// 基础高度：nav_bar(~46) + header(~40) + footer(~42) + padding(~44)
    pub const BASE_HEIGHT: f32 = 172.0;
    /// 每个 quota bar 的预估高度
    pub const PER_QUOTA_HEIGHT: f32 = 42.0;
    /// 最小窗口高度
    pub const MIN_HEIGHT: f32 = 300.0;
    /// 最大窗口高度
    pub const MAX_HEIGHT: f32 = 548.0;
}

/// 根据 quota 数量计算弹出窗口高度（纯函数，适合测试）
pub fn compute_popup_height_for_quotas(quota_count: usize) -> f32 {
    let count = quota_count.max(1) as f32;
    (PopupLayout::BASE_HEIGHT + count * PopupLayout::PER_QUOTA_HEIGHT)
        .clamp(PopupLayout::MIN_HEIGHT, PopupLayout::MAX_HEIGHT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_popup_height_clamps_to_minimum() {
        assert_eq!(compute_popup_height_for_quotas(0), PopupLayout::MIN_HEIGHT);
    }

    #[test]
    fn test_popup_height_single_quota() {
        assert_eq!(compute_popup_height_for_quotas(1), PopupLayout::MIN_HEIGHT);
    }

    #[test]
    fn test_popup_height_three_quotas() {
        let height = compute_popup_height_for_quotas(3);
        let expected = (PopupLayout::BASE_HEIGHT + 3.0 * PopupLayout::PER_QUOTA_HEIGHT)
            .clamp(PopupLayout::MIN_HEIGHT, PopupLayout::MAX_HEIGHT);
        assert!((height - expected).abs() < f32::EPSILON);
        assert!(height >= PopupLayout::MIN_HEIGHT);
        assert!(height <= PopupLayout::MAX_HEIGHT);
    }

    #[test]
    fn test_popup_height_clamps_to_maximum() {
        assert_eq!(compute_popup_height_for_quotas(20), PopupLayout::MAX_HEIGHT);
    }

    #[test]
    fn test_popup_height_monotonically_increases() {
        let mut prev = compute_popup_height_for_quotas(1);
        for n in 2..=8 {
            let h = compute_popup_height_for_quotas(n);
            assert!(h >= prev, "height should be non-decreasing");
            prev = h;
        }
    }
}
