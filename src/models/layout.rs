// ============================================================================
// 弹出窗口布局常量与计算
// ============================================================================

/// 弹出窗口布局相关常量，集中管理避免 magic numbers
///
/// 每个常量对应弹出窗口中一个具体的 UI 区域，可独立审计。
///
/// ## 重要：GPUI 空字符串渲染行为
///
/// 当 `reset_at = None` 时，第4行渲染 `String::new()` 作为 text child，
/// GPUI 仍然会为 `text_size(11)` 的 div 分配行高空间。
/// 因此卡片高度与 `reset_at` 是否存在 **无关**，始终使用统一的 CARD_HEIGHT。
pub struct PopupLayout;

impl PopupLayout {
    /// 弹出窗口固定宽度（px）
    pub const WIDTH: f32 = 380.0;

    // ── 固定区域高度 ──

    /// Header: py(12)×2 + h(36) icon + 1px border-b
    pub const HEADER_HEIGHT: f32 = 61.0;
    /// Nav bar: py(4)×2 + pill_inner(py(6)×2 + 15 line_h) + 1px border-b
    pub const NAV_HEIGHT: f32 = 40.0;
    /// 内容区垂直 padding: pt(10) + pb(8)
    pub const CONTENT_PADDING: f32 = 18.0;
    /// Footer: py(10)×2 + h(38) btn + 1px border-t
    pub const FOOTER_HEIGHT: f32 = 59.0;

    /// 固定区域总高度 (修正: 真实基础偏移拟合增加 13px，合计 195.0)
    pub const FIXED_HEIGHT: f32 =
        Self::HEADER_HEIGHT + Self::NAV_HEIGHT + Self::CONTENT_PADDING + Self::FOOTER_HEIGHT + 13.0;

    // ── Quota 卡片高度 ──

    /// 单个 quota 卡片高度（GPUI 拟合解析结果）
    ///
    /// 经过极限逼近：W(1)∈(360,376)->368，W(2)∈(502,508)->505，W(3)∈(640,644)->642。
    /// 精确得出：卡片高度斜率 CARD_HEIGHT = 129.0。
    pub const CARD_HEIGHT: f32 = 129.0;

    /// 卡片之间的间距: h(8.0) spacer
    pub const CARD_SPACER: f32 = 8.0;

    /// Dashboard 链接行高度: mt(8) + py(10)×2 + icon(16)
    pub const DASHBOARD_ROW_HEIGHT: f32 = 44.0;

    /// 最小窗口高度：1张卡片（不含 dashboard）
    pub const MIN_HEIGHT: f32 = Self::FIXED_HEIGHT + Self::CARD_HEIGHT;
    /// 最大窗口高度
    pub const MAX_HEIGHT: f32 = 720.0;
}

/// 根据 quota 数量和是否有 dashboard 行，计算弹出窗口高度
pub fn compute_popup_height_for_quotas(quota_count: usize) -> f32 {
    compute_popup_height_detailed(quota_count, true)
}

/// 计算弹出窗口高度
pub fn compute_popup_height_detailed(quota_count: usize, has_dashboard: bool) -> f32 {
    let count = quota_count.max(1);

    let cards_height = count as f32 * PopupLayout::CARD_HEIGHT;
    let spacers_height = if count > 1 {
        (count - 1) as f32 * PopupLayout::CARD_SPACER
    } else {
        0.0
    };
    let dashboard_height = if has_dashboard {
        PopupLayout::DASHBOARD_ROW_HEIGHT
    } else {
        0.0
    };

    let raw_height = PopupLayout::FIXED_HEIGHT + cards_height + spacers_height + dashboard_height;

    raw_height.clamp(PopupLayout::MIN_HEIGHT, PopupLayout::MAX_HEIGHT)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// FIXED_HEIGHT 应等于各组件之和 + 13px GPUI 隐式偏移
    #[test]
    fn test_fixed_height_consistency() {
        let sum = PopupLayout::HEADER_HEIGHT
            + PopupLayout::NAV_HEIGHT
            + PopupLayout::CONTENT_PADDING
            + PopupLayout::FOOTER_HEIGHT
            + 13.0; // GPUI 隐式渲染偏移
        assert!(
            (sum - PopupLayout::FIXED_HEIGHT).abs() < f32::EPSILON,
            "FIXED_HEIGHT ({}) should equal sum of parts ({})",
            PopupLayout::FIXED_HEIGHT,
            sum
        );
    }

    #[test]
    fn test_popup_height_single_quota() {
        let h = compute_popup_height_for_quotas(1);
        let expected = PopupLayout::FIXED_HEIGHT
            + PopupLayout::CARD_HEIGHT
            + PopupLayout::DASHBOARD_ROW_HEIGHT;
        assert_eq!(h, expected);
    }

    #[test]
    fn test_popup_height_two_quotas() {
        let h = compute_popup_height_for_quotas(2);
        let expected = PopupLayout::FIXED_HEIGHT
            + 2.0 * PopupLayout::CARD_HEIGHT
            + 1.0 * PopupLayout::CARD_SPACER
            + PopupLayout::DASHBOARD_ROW_HEIGHT;
        assert!((h - expected).abs() < f32::EPSILON);
    }

    #[test]
    fn test_popup_height_three_quotas() {
        let h = compute_popup_height_for_quotas(3);
        let expected = PopupLayout::FIXED_HEIGHT
            + 3.0 * PopupLayout::CARD_HEIGHT
            + 2.0 * PopupLayout::CARD_SPACER
            + PopupLayout::DASHBOARD_ROW_HEIGHT;
        assert!((h - expected).abs() < f32::EPSILON);
        assert!(h <= PopupLayout::MAX_HEIGHT);
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

    #[test]
    fn test_popup_height_without_dashboard() {
        let with = compute_popup_height_detailed(2, true);
        let without = compute_popup_height_detailed(2, false);
        assert!(with > without);
        assert!((with - without - PopupLayout::DASHBOARD_ROW_HEIGHT).abs() < f32::EPSILON);
    }
}
