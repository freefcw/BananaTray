// ============================================================================
// 弹出窗口布局常量与计算
// ============================================================================

/// 弹出窗口布局相关常量，集中管理避免 magic numbers
///
/// 每个常量对应弹出窗口中一个具体的 UI 区域，可独立审计。
///
/// ## 重要：GPUI 空字符串渲染行为
///
/// 当 quota 第4行详情为空时，第4行仍会渲染 `String::new()` 作为 text child，
/// GPUI 仍然会为 `text_size(11)` 的 div 分配行高空间。
/// 因此卡片高度与详情文案是否存在 **无关**，始终使用统一的 CARD_HEIGHT。
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

    /// GPUI 布局引擎隐式偏移（经验拟合值）
    pub const GPUI_IMPLICIT_OFFSET: f32 = 13.0;

    /// 固定区域总高度 (各组件之和 + GPUI 隐式偏移)
    pub const FIXED_HEIGHT: f32 = Self::HEADER_HEIGHT
        + Self::NAV_HEIGHT
        + Self::CONTENT_PADDING
        + Self::FOOTER_HEIGHT
        + Self::GPUI_IMPLICIT_OFFSET;

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

    /// 账户信息卡片高度: py(12)×2 + avatar(44) + border(2) + 底部 spacer(8)
    pub const ACCOUNT_INFO_HEIGHT: f32 = 78.0;

    // ── Overview 紧凑卡片元素尺寸 ──

    /// Overview 状态点宽度
    pub const OVERVIEW_DOT_SIZE: f32 = 6.0;
    /// Overview 元素间距（dot-icon、icon-name 等）
    pub const OVERVIEW_GAP: f32 = 8.0;
    /// Overview Provider 图标尺寸
    pub const OVERVIEW_ICON_SIZE: f32 = 16.0;
    /// Overview 展开态配额行左侧 padding：对齐到图标右侧
    /// dot(6) + gap(8) + icon(16) + gap(8) = 38，但视觉上对齐到图标中心更紧凑
    pub const OVERVIEW_QUOTA_ROW_PL: f32 = 30.0;

    // ── Overview 卡片高度 ──

    /// Overview 单行卡片高度（折叠态 / 1 配额）: py(8)×2 + content(24) + border(2)
    pub const OVERVIEW_ITEM_HEIGHT: f32 = 42.0;
    /// Overview 卡片间距
    pub const OVERVIEW_ITEM_SPACER: f32 = 8.0;

    // ── Overview 共用组件尺寸 ──

    /// 进度条固定宽度（所有布局统一）
    pub const OVERVIEW_BAR_W: f32 = 80.0;
    /// 折叠态/单行进度条高度
    pub const OVERVIEW_BAR_H: f32 = 4.0;
    /// 展开态进度条高度（比折叠态更粗，增强可读性）
    pub const OVERVIEW_EXPANDED_BAR_H: f32 = 6.0;
    /// 数值列固定宽度（确保右侧对齐）
    pub const OVERVIEW_VALUE_W: f32 = 38.0;
    /// 状态徽章列固定宽度
    pub const OVERVIEW_BADGE_W: f32 = 28.0;
    /// 展开/折叠按钮列固定宽度（不可展开时用空白占位保持对齐）
    pub const OVERVIEW_EXPAND_W: f32 = 16.0;

    // ── Overview 展开态行尺寸 ──

    /// 展开态配额行高
    pub const OVERVIEW_QUOTA_LINE_HEIGHT: f32 = 20.0;
    /// 展开态配额行间 gap
    pub const OVERVIEW_QUOTA_LINE_GAP: f32 = 6.0;
    /// 展开态卡片基础高度（不含配额行）: py(8)×2 + header(24) + gap(6) + border(2)
    pub const OVERVIEW_EXPANDED_BASE_HEIGHT: f32 = 48.0;

    /// 最小窗口高度：1张卡片（不含 dashboard）
    pub const MIN_HEIGHT: f32 = Self::FIXED_HEIGHT + Self::CARD_HEIGHT;
    /// 最大窗口高度
    pub const MAX_HEIGHT: f32 = 720.0;
}

/// 根据 quota 数量和是否有 dashboard 行，计算弹出窗口高度
pub fn compute_popup_height_for_quotas(quota_count: usize) -> f32 {
    compute_popup_height_detailed(quota_count, true, false)
}

/// 根据已启用 Provider 数量计算 Overview 面板的弹出窗口高度
///
/// 所有卡片默认折叠为单行，展开后靠内容区 scroll 处理。
pub fn compute_popup_height_for_overview(provider_count: usize) -> f32 {
    if provider_count == 0 {
        return PopupLayout::FIXED_HEIGHT + PopupLayout::OVERVIEW_ITEM_HEIGHT;
    }
    let cards = provider_count as f32 * PopupLayout::OVERVIEW_ITEM_HEIGHT;
    let spacers = if provider_count > 1 {
        (provider_count - 1) as f32 * PopupLayout::OVERVIEW_ITEM_SPACER
    } else {
        0.0
    };
    let raw = PopupLayout::FIXED_HEIGHT + cards + spacers;
    raw.clamp(PopupLayout::MIN_HEIGHT, PopupLayout::MAX_HEIGHT)
}

impl PopupLayout {
    /// 计算展开态多行布局的卡片高度
    pub fn overview_multi_item_height(quota_rows: usize) -> f32 {
        let rows = quota_rows.max(1);
        let lines_height = rows as f32 * Self::OVERVIEW_QUOTA_LINE_HEIGHT;
        let lines_gap = if rows > 1 {
            (rows - 1) as f32 * Self::OVERVIEW_QUOTA_LINE_GAP
        } else {
            0.0
        };
        Self::OVERVIEW_EXPANDED_BASE_HEIGHT + lines_height + lines_gap
    }
}

/// 计算弹出窗口高度（统一所有可选区域的高度因素）
pub fn compute_popup_height_detailed(
    quota_count: usize,
    has_dashboard: bool,
    has_account_info: bool,
) -> f32 {
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
    let account_height = if has_account_info {
        PopupLayout::ACCOUNT_INFO_HEIGHT
    } else {
        0.0
    };

    let raw_height = PopupLayout::FIXED_HEIGHT
        + cards_height
        + spacers_height
        + dashboard_height
        + account_height;

    raw_height.clamp(PopupLayout::MIN_HEIGHT, PopupLayout::MAX_HEIGHT)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// FIXED_HEIGHT 应等于各组件之和 + GPUI 隐式偏移
    #[test]
    fn test_fixed_height_consistency() {
        let sum = PopupLayout::HEADER_HEIGHT
            + PopupLayout::NAV_HEIGHT
            + PopupLayout::CONTENT_PADDING
            + PopupLayout::FOOTER_HEIGHT
            + PopupLayout::GPUI_IMPLICIT_OFFSET;
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
        let with = compute_popup_height_detailed(2, true, false);
        let without = compute_popup_height_detailed(2, false, false);
        assert!(with > without);
        assert!((with - without - PopupLayout::DASHBOARD_ROW_HEIGHT).abs() < f32::EPSILON);
    }

    #[test]
    fn test_popup_height_with_account_info() {
        let without = compute_popup_height_detailed(2, true, false);
        let with = compute_popup_height_detailed(2, true, true);
        assert!(with > without);
        assert!((with - without - PopupLayout::ACCOUNT_INFO_HEIGHT).abs() < f32::EPSILON);
    }

    // ── Overview 高度计算 ──────────────────────────────────

    #[test]
    fn overview_height_empty_has_minimum() {
        let h = compute_popup_height_for_overview(0);
        let expected = PopupLayout::FIXED_HEIGHT + PopupLayout::OVERVIEW_ITEM_HEIGHT;
        assert!((h - expected).abs() < f32::EPSILON);
    }

    #[test]
    fn overview_height_single_provider() {
        let h = compute_popup_height_for_overview(1);
        let raw = PopupLayout::FIXED_HEIGHT + PopupLayout::OVERVIEW_ITEM_HEIGHT;
        let expected = raw.clamp(PopupLayout::MIN_HEIGHT, PopupLayout::MAX_HEIGHT);
        assert!((h - expected).abs() < f32::EPSILON);
    }

    #[test]
    fn overview_height_multiple_providers() {
        let h = compute_popup_height_for_overview(3);
        let expected = PopupLayout::FIXED_HEIGHT
            + 3.0 * PopupLayout::OVERVIEW_ITEM_HEIGHT
            + 2.0 * PopupLayout::OVERVIEW_ITEM_SPACER;
        assert!((h - expected).abs() < f32::EPSILON);
    }

    #[test]
    fn overview_height_clamps_to_max() {
        let h = compute_popup_height_for_overview(100);
        assert_eq!(h, PopupLayout::MAX_HEIGHT);
    }

    // ── 展开态多行高度 ──────────────────────────────────

    #[test]
    fn multi_item_height_increases_with_rows() {
        let h1 = PopupLayout::overview_multi_item_height(1);
        let h2 = PopupLayout::overview_multi_item_height(2);
        let h3 = PopupLayout::overview_multi_item_height(3);
        assert!(h2 > h1);
        assert!(h3 > h2);
    }

    #[test]
    fn multi_item_height_single_row_matches_formula() {
        // base(48) + 1×line(20) + 0×gap = 68
        let h = PopupLayout::overview_multi_item_height(1);
        assert!((h - 68.0).abs() < f32::EPSILON);
    }

    #[test]
    fn multi_item_height_two_rows_matches_formula() {
        // base(48) + 2×line(20) + 1×gap(6) = 94
        let h = PopupLayout::overview_multi_item_height(2);
        assert!((h - 94.0).abs() < f32::EPSILON);
    }
}
