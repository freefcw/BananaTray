use crate::application::AppAction;
use crate::models::NavTab;
use crate::runtime;
use crate::theme::Theme;
use crate::ui::AppView;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use std::time::Duration;

/// 两侧指示器区域宽度
const INDICATOR_WIDTH: f32 = 14.0;
/// 滚动偏移量超过此阈值才显示指示器
const SCROLL_THRESHOLD: f32 = 2.0;
/// 滑块动画时长 (ms)
const SLIDER_ANIMATION_MS: u64 = 450;

/// 果冻缓动：在 animator 闭包内使用，将 0→1 的 delta 映射成带过冲的值。
/// 前 60% 时间冲到目标的 ~106%，后 40% 回弹到 100%。
fn jelly_overshoot(t: f32) -> f32 {
    let overshoot = 1.06;
    if t < 0.6 {
        let p = t / 0.6;
        // ease-out-cubic 到 overshoot
        let ease = 1.0 - (1.0 - p).powi(3);
        ease * overshoot
    } else {
        let p = (t - 0.6) / 0.4;
        // ease-in-out 从 overshoot 回到 1.0
        let ease = p * p * (3.0 - 2.0 * p); // smoothstep
        overshoot + (1.0 - overshoot) * ease
    }
}

/// 线性插值
fn lerp(a: Pixels, b: Pixels, t: f32) -> Pixels {
    a + (b - a) * t
}

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
        let generation = state_ref.session.nav.generation;
        let prev_tab = state_ref.session.nav.prev_active_tab.clone();
        let custom_ids = state_ref.session.provider_store.custom_provider_ids();
        drop(state_ref);
        let ordered_ids = settings.provider.ordered_provider_ids(&custom_ids);
        let nav_items: Vec<_> = ordered_ids
            .iter()
            .filter(|id| settings.provider.is_enabled(id))
            .filter_map(|id| {
                providers.iter().find(|p| p.provider_id == *id).map(|p| {
                    (
                        p.icon_asset().to_string(),
                        p.display_name().to_string(),
                        NavTab::Provider(id.clone()),
                    )
                })
            })
            .collect();

        // 计算 active / prev pill 在 nav_items 中的索引
        let active_index = nav_items.iter().position(|(_, _, tab)| *tab == active_tab);
        let prev_index =
            prev_tab.and_then(|pt| nav_items.iter().position(|(_, _, tab)| *tab == pt));

        let border_color = theme.border.subtle;

        let offset = self.nav_scroll_handle.offset();
        let max_offset = self.nav_scroll_handle.max_offset();
        let threshold = px(SCROLL_THRESHOLD);
        let can_scroll_left = offset.x < threshold.negate();
        let can_scroll_right = max_offset.width > threshold && offset.x > max_offset.width.negate();

        let indicator_color = theme.text.muted;

        // 计算点击箭头后要滚动到的目标 item 索引
        let scroll_handle = self.nav_scroll_handle.clone();
        let scroll_handle_r = scroll_handle.clone();
        let entity_l = cx.entity().clone();
        let entity_r = cx.entity().clone();

        // 找到左侧第一个被裁剪/隐藏的 item 索引
        let left_target = Self::find_left_target(&self.nav_scroll_handle);
        // 找到右侧第一个被裁剪/隐藏的 item 索引
        let right_target = Self::find_right_target(&self.nav_scroll_handle);

        // ── 滑块位置计算 ──
        // bounds_for_item 返回的是 layout bounds（内容空间，不含 scroll offset）。
        // 滑块放在 scroll 容器的外层 wrapper 中（overflow_hidden + relative），
        // 需要：1) 减去 scroll 容器的 left 得到内容空间中的相对坐标
        //       2) 加上 scroll offset.x 转换为视觉位置
        let scroll_bounds = self.nav_scroll_handle.bounds();
        let scroll_left = scroll_bounds.left();
        let scroll_offset_x = offset.x; // 已在上面获取（负值=向左滚动过）

        // 将 layout bounds 转换为滑块在 wrapper 中的视觉坐标 (left, width, height)
        let to_visual = |b: Bounds<Pixels>| -> (Pixels, Pixels, Pixels) {
            (
                b.left() - scroll_left + scroll_offset_x,
                b.size.width,
                b.size.height,
            )
        };

        // 滑块的目标位置（当前 active pill）
        let target_rect = active_index
            .and_then(|ix| self.nav_scroll_handle.bounds_for_item(ix))
            .map(to_visual);

        // 滑块的起始位置（上一个 active pill）
        let from_rect = prev_index
            .and_then(|ix| self.nav_scroll_handle.bounds_for_item(ix))
            .map(to_visual);

        let slider_bg = theme.nav.pill_active_bg;

        // ── 构建滑块元素 ──
        let slider = target_rect
            .map(|_| self.render_nav_slider(target_rect, from_rect, generation, slider_bg));

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
                // ── 中间区域：relative wrapper + absolute 滑块 + scroll 容器 ──
                // 滑块放在 wrapper 内（不在 scroll 容器内），避免影响 scroll children 索引
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .relative()
                    // 滑块背景层（absolute，z-order 在 scroll 之下）
                    .when_some(slider, |el, s| el.child(s))
                    .child(
                        div()
                            .id("nav-provider-scroll")
                            .overflow_x_scroll()
                            .scrollbar_width(px(0.0))
                            .flex()
                            .items_center()
                            .gap(px(2.0))
                            .track_scroll(&self.nav_scroll_handle)
                            .children(nav_items.into_iter().map(|(icon, label, tab)| {
                                self.render_nav_pill(icon, label, tab, active_tab.clone(), cx)
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

    /// 渲染导航栏滑块背景（absolute 定位，带果冻动画）
    fn render_nav_slider(
        &self,
        target_rect: Option<(Pixels, Pixels, Pixels)>,
        from_rect: Option<(Pixels, Pixels, Pixels)>,
        generation: u64,
        bg: Hsla,
    ) -> impl IntoElement {
        let (to_left, to_width, to_height) = target_rect.unwrap();

        // 如果有 from_rect 且和 target 不同 → 播放动画
        // 否则直接定位到 target（无动画）
        let should_animate = from_rect
            .map(|(fl, fw, _)| (fl - to_left).abs() > px(1.0) || (fw - to_width).abs() > px(1.0))
            .unwrap_or(false);

        let base = div()
            .absolute()
            .top(px(0.0))
            .h(to_height)
            .rounded(px(8.0))
            .bg(bg);

        if should_animate {
            let (from_left, from_width, _) = from_rect.unwrap();

            base.with_animation(
                ElementId::Name(format!("nav-slider-{}", generation).into()),
                Animation::new(Duration::from_millis(SLIDER_ANIMATION_MS)),
                move |el, delta| {
                    // delta: 0.0 → 1.0 (linear)
                    // 应用果冻过冲映射
                    let t = jelly_overshoot(delta);
                    let left = lerp(from_left, to_left, t);
                    let width = lerp(from_width, to_width, t);
                    el.left(left).w(width)
                },
            )
            .into_any_element()
        } else {
            base.left(to_left).w(to_width).into_any_element()
        }
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

    /// Lumina Bar 风格的 pill tab：水平 icon + label
    /// 选中状态的背景由滑块提供，pill 本身始终透明背景
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

        let (text_color, icon_color) = if is_active {
            (theme.nav.pill_active_text, theme.nav.pill_active_text)
        } else {
            (theme.text.muted, theme.text.muted)
        };

        div()
            .flex()
            .items_center()
            .gap(px(5.0))
            .px(px(10.0))
            .py(px(6.0))
            .rounded(px(8.0))
            .cursor_pointer()
            // pill 本身不设背景，由滑块层提供
            .hover(|style| {
                if is_active {
                    style
                } else {
                    style.bg(theme.bg.subtle)
                }
            })
            .child(crate::ui::widgets::render_provider_icon(
                icon_path,
                px(15.0),
                icon_color,
            ))
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
                    runtime::dispatch_in_context(
                        &view.state,
                        AppAction::SelectNavTab(tab.clone()),
                        cx,
                    );
                });
            })
    }
}
