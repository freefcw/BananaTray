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
    /// 这个高度不含内嵌折线。有折线的卡片另加 `CARD_HISTORY_EXTRA`。
    pub const CARD_HEIGHT: f32 = 129.0;

    /// 配额卡内嵌折线的绘图区高度，不含坐标文字。
    pub const CARD_HISTORY_PLOT_HEIGHT: f32 = 44.0;
    /// 横轴时间行，以及它和绘图区之间的间距。纵轴不单独占一行。
    pub const CARD_HISTORY_AXIS_TIME: f32 = 16.0;
    pub const CARD_HISTORY_AXIS_GAP: f32 = 4.0;
    /// 绘图框内左侧留给纵轴数字的宽度。折线从这列右边起笔，横轴起点和折线对齐。
    /// 绘图框本身不再为这列往右退。
    pub const CARD_HISTORY_Y_GUTTER: f32 = 44.0;
    pub const CARD_HISTORY_AXIS_CHROME: f32 =
        Self::CARD_HISTORY_AXIS_TIME + Self::CARD_HISTORY_AXIS_GAP;
    /// 绘图区 + 坐标文字。设置页只复用坐标尺寸，绘图区更高。
    pub const CARD_HISTORY_CHART_HEIGHT: f32 =
        Self::CARD_HISTORY_PLOT_HEIGHT + Self::CARD_HISTORY_AXIS_CHROME;

    /// 线条样式点开放大后的绘图区高度，不含坐标文字。
    pub const CARD_HISTORY_EXPANDED_PLOT_HEIGHT: f32 = 110.0;
    /// 放大的折线比平时多出的高度。
    pub const CARD_HISTORY_EXPANDED_EXTRA: f32 =
        Self::CARD_HISTORY_EXPANDED_PLOT_HEIGHT - Self::CARD_HISTORY_PLOT_HEIGHT;

    /// 一张有折线的配额卡比 `CARD_HEIGHT` 多出来的高度。
    ///
    /// 线条样式把折线放在卡片底部：卡片是 `flex_col` + `gap(6)`，多一个图表子节点就多一段
    /// 6px 间距，再加上图表自身高度。圆圈样式把折线放在圆环右侧，详情在顶栏，不再加这段。
    pub const CARD_HISTORY_EXTRA: f32 = 6.0 + Self::CARD_HISTORY_CHART_HEIGHT;

    /// 卡片之间的间距: h(8.0) spacer
    pub const CARD_SPACER: f32 = 8.0;

    /// 限流重置卡上内边距。
    pub const RESET_CARD_PAD_TOP: f32 = 12.0;
    /// 限流重置卡下内边距，与上内边距对称。
    pub const RESET_CARD_PAD_BOTTOM: f32 = 12.0;
    /// 上下内边距合计，高度计算用这一份。
    pub const RESET_CARD_PAD_Y: f32 = Self::RESET_CARD_PAD_TOP + Self::RESET_CARD_PAD_BOTTOM;
    /// 重置卡 1px 边框的上下合计。
    pub const RESET_CARD_BORDER: f32 = 2.0;
    /// 重置卡标题、类型/时间行、数量说明行的固定行高。
    pub const RESET_LINE_HEIGHT: f32 = 20.0;
    /// 标题、每一条重置、数量说明之间的间距。
    pub const RESET_CARD_GAP: f32 = 8.0;
    /// 一条重置的紧凑单行高度。
    pub const RESET_ITEM_HEIGHT: f32 = Self::RESET_LINE_HEIGHT;

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
    /// Overview 展开态配额行左侧 padding：对齐到 Provider 图标左边缘（dot(6) + gap(8) = 14）
    pub const OVERVIEW_QUOTA_ROW_PL: f32 = Self::OVERVIEW_DOT_SIZE + Self::OVERVIEW_GAP;

    // ── Overview 卡片高度 ──

    /// Overview 单行卡片高度（折叠态 / 1 配额）: py(8)×2 + content(24) + border(2)
    pub const OVERVIEW_ITEM_HEIGHT: f32 = 42.0;
    /// Overview 卡片间距
    pub const OVERVIEW_ITEM_SPACER: f32 = 8.0;

    // ── Overview 共用组件尺寸 ──

    /// 进度条固定宽度（所有布局统一）
    pub const OVERVIEW_BAR_W: f32 = 72.0;
    /// 折叠态/单行进度条高度
    pub const OVERVIEW_BAR_H: f32 = 4.0;
    /// 展开态进度条高度（比折叠态更粗，增强可读性）
    pub const OVERVIEW_EXPANDED_BAR_H: f32 = 6.0;
    /// 展开/折叠按钮方形点击区域（不可展开时用等宽空白占位保持对齐）
    pub const OVERVIEW_EXPAND_W: f32 = 24.0;

    // ── Overview 展开态行尺寸 ──

    /// 展开态配额行高
    pub const OVERVIEW_QUOTA_LINE_HEIGHT: f32 = 20.0;
    /// 展开态配额行间 gap
    pub const OVERVIEW_QUOTA_LINE_GAP: f32 = 6.0;
    /// 展开态卡片基础高度（不含配额行）: py(8)×2 + header(24) + gap(6) + border(2)
    pub const OVERVIEW_EXPANDED_BASE_HEIGHT: f32 = 48.0;

    /// 最小窗口高度：1张卡片（不含 dashboard）
    pub const MIN_HEIGHT: f32 = Self::FIXED_HEIGHT + Self::CARD_HEIGHT;
    /// Overview 面板的最小窗口高度：固定区域 + 1 张紧凑卡片。
    ///
    /// 不复用 `MIN_HEIGHT`（含一张 129px 详情大卡）：单 Provider 的 Overview
    /// 内容只有一张 42px 紧凑卡，套用 `MIN_HEIGHT` 会产生 ~87px 死空白。
    pub const MIN_OVERVIEW_HEIGHT: f32 = Self::FIXED_HEIGHT + Self::OVERVIEW_ITEM_HEIGHT;
    /// 最大窗口高度
    pub const MAX_HEIGHT: f32 = 720.0;
}

/// 限流重置卡高度。没有明细也没有数量说明时为 0。
///
/// 行高和 `ui/widgets/display/reset_card.rs` 使用同一组常量，避免弹窗裁切。
pub fn reset_card_height(detail_rows: usize, summary_line: bool) -> f32 {
    if detail_rows == 0 && !summary_line {
        return 0.0;
    }

    let mut pieces = 1.0;
    let mut content = PopupLayout::RESET_LINE_HEIGHT;
    if detail_rows > 0 {
        pieces += detail_rows as f32;
        content += detail_rows as f32 * PopupLayout::RESET_ITEM_HEIGHT;
    }
    if summary_line {
        pieces += 1.0;
        content += PopupLayout::RESET_LINE_HEIGHT;
    }
    let gaps = (pieces - 1.0) * PopupLayout::RESET_CARD_GAP;
    PopupLayout::RESET_CARD_PAD_Y + PopupLayout::RESET_CARD_BORDER + content + gaps
}

/// 重置卡加上它和上方内容之间的 8px 间距。
pub fn reset_popup_extra(detail_rows: usize, summary_line: bool) -> f32 {
    let card = reset_card_height(detail_rows, summary_line);
    if card <= 0.0 {
        0.0
    } else {
        PopupLayout::CARD_SPACER + card
    }
}

/// 根据 quota 数量和是否有 dashboard 行，计算弹出窗口高度
pub fn compute_popup_height_for_quotas(quota_count: usize) -> f32 {
    compute_popup_height_detailed(quota_count, true, false)
}

/// 根据每个已启用 Provider 的 Overview 卡片行数计算弹出窗口高度
///
/// `card_rows` 按 Provider 顺序给出各卡片的配额行数：`1` 为折叠/单配额的单行卡片，
/// `>1` 为展开态多行卡片。超过 `MAX_HEIGHT` 的部分仍靠内容区 scroll 处理。
pub fn compute_popup_height_for_overview(card_rows: &[usize]) -> f32 {
    if card_rows.is_empty() {
        return PopupLayout::MIN_OVERVIEW_HEIGHT;
    }
    let cards: f32 = card_rows
        .iter()
        .map(|rows| {
            if *rows > 1 {
                PopupLayout::overview_multi_item_height(*rows)
            } else {
                PopupLayout::OVERVIEW_ITEM_HEIGHT
            }
        })
        .sum();
    let spacers = card_rows.len().saturating_sub(1) as f32 * PopupLayout::OVERVIEW_ITEM_SPACER;
    let raw = PopupLayout::FIXED_HEIGHT + cards + spacers;
    raw.clamp(PopupLayout::MIN_OVERVIEW_HEIGHT, PopupLayout::MAX_HEIGHT)
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

    #[test]
    fn reset_card_height_counts_type_and_both_timestamps() {
        assert_eq!(reset_card_height(0, false), 0.0);
        let one = reset_card_height(1, false);
        let expected = PopupLayout::RESET_CARD_PAD_Y
            + PopupLayout::RESET_CARD_BORDER
            + PopupLayout::RESET_LINE_HEIGHT
            + PopupLayout::RESET_CARD_GAP
            + PopupLayout::RESET_ITEM_HEIGHT;
        assert!((one - expected).abs() < f32::EPSILON);
        assert!(
            (reset_popup_extra(1, false) - (PopupLayout::CARD_SPACER + one)).abs() < f32::EPSILON
        );
        assert!(reset_card_height(1, true) > one);
    }

    // ── Overview 高度计算 ──────────────────────────────────

    #[test]
    fn overview_height_empty_has_minimum() {
        let h = compute_popup_height_for_overview(&[]);
        let expected = PopupLayout::FIXED_HEIGHT + PopupLayout::OVERVIEW_ITEM_HEIGHT;
        assert!((h - expected).abs() < f32::EPSILON);
    }

    #[test]
    fn overview_height_single_provider() {
        let h = compute_popup_height_for_overview(&[1]);
        let raw = PopupLayout::FIXED_HEIGHT + PopupLayout::OVERVIEW_ITEM_HEIGHT;
        let expected = raw.clamp(PopupLayout::MIN_OVERVIEW_HEIGHT, PopupLayout::MAX_HEIGHT);
        assert!((h - expected).abs() < f32::EPSILON);
        // 单 Provider 时窗口紧贴内容，不应多出死空白
        assert!((h - raw).abs() < f32::EPSILON);
    }

    #[test]
    fn overview_height_multiple_providers() {
        let h = compute_popup_height_for_overview(&[1, 1, 1]);
        let expected = PopupLayout::FIXED_HEIGHT
            + 3.0 * PopupLayout::OVERVIEW_ITEM_HEIGHT
            + 2.0 * PopupLayout::OVERVIEW_ITEM_SPACER;
        assert!((h - expected).abs() < f32::EPSILON);
    }

    #[test]
    fn overview_height_clamps_to_max() {
        let h = compute_popup_height_for_overview(&[1; 100]);
        assert_eq!(h, PopupLayout::MAX_HEIGHT);
    }

    /// 展开态卡片按多行高度计算，窗口跟着长高（否则重开弹窗会立刻溢出滚动）
    #[test]
    fn overview_height_expanded_card_is_taller() {
        let collapsed = compute_popup_height_for_overview(&[1, 1]);
        let expanded = compute_popup_height_for_overview(&[3, 1]);
        let expected = PopupLayout::FIXED_HEIGHT
            + PopupLayout::overview_multi_item_height(3)
            + PopupLayout::OVERVIEW_ITEM_HEIGHT
            + PopupLayout::OVERVIEW_ITEM_SPACER;
        assert!((expanded - expected).abs() < f32::EPSILON);
        assert!(expanded > collapsed);
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
