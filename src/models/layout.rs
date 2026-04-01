// ============================================================================
// 弹出窗口布局常量与计算
// ============================================================================

/// 弹出窗口布局相关常量，集中管理避免 magic numbers
///
/// ## 高度组成明细
///
/// | 区域            | 来源                                    | 高度(px) |
/// |-----------------|-----------------------------------------|----------|
/// | Header          | py(12)×2 + 36 icon + 1 border-b         | ~61      |
/// | Nav bar         | py(4)×2 + pill(py(6)×2+15+border) + 1   | ~39      |
/// | Content padding | pt(10) + pb(12)                         | ~22      |
/// | Footer          | py(10)×2 + 38 btn + 1 border-t          | ~59      |
/// | Dashboard row   | mt(8) + py(10)×2 + 16 icon              | ~44      |
///
/// Quota 卡片 ≈ 140px，卡片间距由容器 gap(16) 控制，每槽位需 ~156px
pub struct PopupLayout;

impl PopupLayout {
    /// 弹出窗口固定宽度（px）
    pub const WIDTH: f32 = 380.0;
    /// 基础高度 = 固定区域(~160) + 内容开销(~44 dashboard + ~22 padding)
    /// 略偏保守，宁可内容可滚动也不留大量空白
    pub const BASE_HEIGHT: f32 = 204.0;
    /// 每个 quota 卡片的高度（~140px 卡片 + 8px spacer + 余量）
    pub const PER_QUOTA_HEIGHT: f32 = 152.0;
    /// 最小窗口高度
    pub const MIN_HEIGHT: f32 = 380.0;
    /// 最大窗口高度
    pub const MAX_HEIGHT: f32 = 720.0;
}

/// 根据 quota 数量计算弹出窗口高度（纯函数，适合测试）
///
/// 高度 = BASE + N × PER_QUOTA，clamp 到 [MIN, MAX]
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
        // 0 quotas treated as 1 → 204 + 152 = 356 → clamp to 380
        assert_eq!(compute_popup_height_for_quotas(0), PopupLayout::MIN_HEIGHT);
    }

    #[test]
    fn test_popup_height_single_quota() {
        // 204 + 152 = 356 → clamp to 380
        assert_eq!(compute_popup_height_for_quotas(1), PopupLayout::MIN_HEIGHT);
    }

    #[test]
    fn test_popup_height_two_quotas() {
        // 204 + 304 = 508
        let height = compute_popup_height_for_quotas(2);
        let expected = PopupLayout::BASE_HEIGHT + 2.0 * PopupLayout::PER_QUOTA_HEIGHT;
        assert!((height - expected).abs() < f32::EPSILON);
    }

    #[test]
    fn test_popup_height_three_quotas() {
        // 204 + 456 = 660
        let height = compute_popup_height_for_quotas(3);
        let expected = PopupLayout::BASE_HEIGHT + 3.0 * PopupLayout::PER_QUOTA_HEIGHT;
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
