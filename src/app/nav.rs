use super::AppView;
use crate::application::AppAction;
use crate::models::NavTab;
use crate::runtime;
use crate::theme::Theme;
use gpui::prelude::FluentBuilder as _;
use gpui::*;

/// 两侧指示器区域宽度
const INDICATOR_WIDTH: f32 = 14.0;
/// 滚动偏移量超过此阈值才显示指示器
const SCROLL_THRESHOLD: f32 = 2.0;

impl AppView {
    pub(crate) fn render_top_nav(
        &self,
        active_tab: NavTab,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.global::<Theme>();
        let state_ref = self.state.borrow();
        let settings = state_ref.session.settings.clone();
        let providers = state_ref.session.provider_store.providers.clone();
        drop(state_ref);

        let provider_order = settings.ordered_providers();
        let nav_items: Vec<_> = provider_order
            .into_iter()
            .filter(|kind| settings.is_provider_enabled(*kind))
            .filter_map(|kind| {
                providers.iter().find(|p| p.kind == kind).map(|p| {
                    (
                        p.icon_asset().to_string(),
                        p.display_name().to_string(),
                        NavTab::Provider(kind),
                    )
                })
            })
            .collect();

        let border_color = theme.border_subtle;

        let offset = self.nav_scroll_handle.offset();
        let max_offset = self.nav_scroll_handle.max_offset();
        let threshold = px(SCROLL_THRESHOLD);
        let can_scroll_left = offset.x < threshold.negate();
        let can_scroll_right = max_offset.width > threshold && offset.x > max_offset.width.negate();

        let indicator_color = theme.text_muted;

        // 计算点击箭头后要滚动到的目标 item 索引
        let scroll_handle = self.nav_scroll_handle.clone();
        let scroll_handle_r = scroll_handle.clone();
        let entity_l = cx.entity().clone();
        let entity_r = cx.entity().clone();

        // 找到左侧第一个被裁剪/隐藏的 item 索引
        let left_target = Self::find_left_target(&self.nav_scroll_handle);
        // 找到右侧第一个被裁剪/隐藏的 item 索引
        let right_target = Self::find_right_target(&self.nav_scroll_handle);

        div()
            .w_full()
            .border_b_1()
            .border_color(border_color)
            .py(px(4.0))
            .flex()
            .items_center()
            .child(
                // ── 左侧箭头指示器 ──
                div()
                    .id("nav-arrow-left")
                    .w(px(INDICATOR_WIDTH))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .when(can_scroll_left, |el| {
                        el.child(
                            svg()
                                .path("src/icons/chevron-left.svg")
                                .size(px(10.0))
                                .text_color(indicator_color),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            move |_, _, cx| {
                                if let Some(ix) = left_target {
                                    scroll_handle.scroll_to_item(ix);
                                }
                                entity_l.update(cx, |_, cx| cx.notify());
                            },
                        )
                    }),
            )
            .child(
                // ── 中间滚动区域 ──
                div().flex_1().min_w_0().overflow_hidden().child(
                    div()
                        .id("nav-provider-scroll")
                        .overflow_x_scroll()
                        .scrollbar_width(px(0.0))
                        .flex()
                        .items_center()
                        .gap(px(2.0))
                        .track_scroll(&self.nav_scroll_handle)
                        .children(nav_items.into_iter().map(|(icon, label, tab)| {
                            self.render_nav_pill(icon, label, tab, active_tab, cx)
                        })),
                ),
            )
            .child(
                // ── 右侧箭头指示器 ──
                div()
                    .id("nav-arrow-right")
                    .w(px(INDICATOR_WIDTH))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .when(can_scroll_right, |el| {
                        el.child(
                            svg()
                                .path("src/icons/chevron-right.svg")
                                .size(px(10.0))
                                .text_color(indicator_color),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            move |_, _, cx| {
                                if let Some(ix) = right_target {
                                    scroll_handle_r.scroll_to_item(ix);
                                }
                                entity_r.update(cx, |_, cx| cx.notify());
                            },
                        )
                    }),
            )
    }

    /// 找到当前可见区域左边缘之前的一个 item（向左滚动目标）
    fn find_left_target(handle: &ScrollHandle) -> Option<usize> {
        let offset = handle.offset();
        let bounds = handle.bounds();
        // 可见区域的左边界（在内容坐标系中）
        let visible_left = bounds.left() - offset.x;
        let count = handle.children_count();

        // 从左向右找到第一个 left >= visible_left 的 item，目标是它前面一个
        for i in 0..count {
            if let Some(cb) = handle.bounds_for_item(i) {
                if cb.left() >= visible_left - px(1.0) {
                    return Some(i.saturating_sub(1));
                }
            }
        }
        None
    }

    /// 找到当前可见区域右边缘之后的一个 item（向右滚动目标）
    fn find_right_target(handle: &ScrollHandle) -> Option<usize> {
        let offset = handle.offset();
        let bounds = handle.bounds();
        // 可见区域的右边界（在内容坐标系中）
        let visible_right = bounds.right() - offset.x;
        let count = handle.children_count();

        // 从左向右找到第一个 right > visible_right 的 item
        for i in 0..count {
            if let Some(cb) = handle.bounds_for_item(i) {
                if cb.right() > visible_right + px(1.0) {
                    return Some(i);
                }
            }
        }
        // 如果没找到，滚动到最后一个
        if count > 0 {
            Some(count - 1)
        } else {
            None
        }
    }

    /// Lumina Bar 风格的 pill tab：水平 icon + label，选中时高亮背景
    fn render_nav_pill(
        &self,
        icon_path: String,
        label: String,
        tab: NavTab,
        active_tab: NavTab,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let is_active = tab == active_tab;
        let theme = cx.global::<Theme>();
        let entity = cx.entity().clone();

        let (bg, text_color, icon_color) = if is_active {
            (
                theme.nav_pill_active_bg,
                theme.nav_pill_active_text,
                theme.nav_pill_active_text,
            )
        } else {
            (transparent_black(), theme.text_muted, theme.text_muted)
        };

        let border_color = if is_active {
            theme.nav_pill_active_bg
        } else {
            transparent_black()
        };

        div()
            .flex()
            .items_center()
            .gap(px(5.0))
            .px(px(10.0))
            .py(px(6.0))
            .rounded(px(8.0))
            .cursor_pointer()
            .bg(bg)
            .border_1()
            .border_color(border_color)
            .hover(|style| {
                if is_active {
                    style
                } else {
                    style.bg(theme.bg_subtle)
                }
            })
            .child(
                svg()
                    .path(icon_path)
                    .size(px(15.0))
                    .text_color(icon_color)
                    .flex_shrink_0(),
            )
            .child(
                div()
                    .text_size(px(13.0))
                    .line_height(relative(1.2))
                    .font_weight(if is_active {
                        FontWeight::SEMIBOLD
                    } else {
                        FontWeight::MEDIUM
                    })
                    .text_color(text_color)
                    .child(label),
            )
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                entity.update(cx, |view, cx| {
                    runtime::dispatch_in_context(&view.state, AppAction::SelectNavTab(tab), cx);
                });
            })
    }
}
