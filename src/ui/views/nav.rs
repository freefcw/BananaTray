use crate::application::AppAction;
use crate::models::NavTab;
use crate::runtime;
use crate::theme::Theme;
use crate::ui::AppView;
use gpui::{
    div, px, relative, svg, Animation, AnimationExt, Bounds, Context, Div, ElementId, Entity,
    FontWeight, Hsla, InteractiveElement, IntoElement, MouseButton, Negate, ParentElement, Pixels,
    ScrollHandle, StatefulInteractiveElement, StyleRefinement, Styled,
};
use rust_i18n::t;
use std::time::Duration;

/// Overview 导航图标路径（与 tray_settings.rs 保持一致）
const OVERVIEW_ICON: &str = "src/icons/overview.svg";
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

struct TopNavItem {
    icon_path: String,
    label: String,
    tab: NavTab,
}

struct TopNavSnapshot {
    items: Vec<TopNavItem>,
    previous_tab: Option<NavTab>,
    generation: u64,
}

impl TopNavSnapshot {
    fn active_index(&self, active_tab: &NavTab) -> Option<usize> {
        self.items.iter().position(|item| item.tab == *active_tab)
    }

    fn previous_index(&self) -> Option<usize> {
        self.previous_tab
            .as_ref()
            .and_then(|tab| self.items.iter().position(|item| item.tab == *tab))
    }
}

#[derive(Clone, Copy)]
struct NavSliderRect {
    left: Pixels,
    width: Pixels,
    height: Pixels,
}

#[derive(Clone, Copy)]
struct NavSliderLayout {
    target: NavSliderRect,
    from: Option<NavSliderRect>,
}

struct NavScrollMetrics {
    can_scroll_left: bool,
    can_scroll_right: bool,
    left_target: Option<usize>,
    right_target: Option<usize>,
    slider_layout: Option<NavSliderLayout>,
}

impl AppView {
    pub(crate) fn render_top_nav(
        &self,
        active_tab: NavTab,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.global::<Theme>();
        let border_color = theme.border.subtle;
        let indicator_color = theme.text.muted;
        let slider_bg = theme.nav.pill_active_bg;
        let snapshot = self.top_nav_snapshot();
        let active_index = snapshot.active_index(&active_tab);
        let previous_index = snapshot.previous_index();
        let metrics =
            Self::measure_nav_scroll(&self.nav_scroll_handle, active_index, previous_index);

        let left_arrow = Self::render_nav_scroll_arrow(
            "nav-arrow-left",
            "src/icons/chevron-left.svg",
            metrics.can_scroll_left,
            metrics.left_target,
            self.nav_scroll_handle.clone(),
            cx.entity().clone(),
            indicator_color,
        );
        let right_arrow = Self::render_nav_scroll_arrow(
            "nav-arrow-right",
            "src/icons/chevron-right.svg",
            metrics.can_scroll_right,
            metrics.right_target,
            self.nav_scroll_handle.clone(),
            cx.entity().clone(),
            indicator_color,
        );
        let slider = metrics
            .slider_layout
            .map(|layout| self.render_nav_slider(layout, snapshot.generation, slider_bg));

        let nav_scroll = div()
            .id("nav-provider-scroll")
            .overflow_x_scroll()
            .scrollbar_width(px(0.0))
            .flex()
            .items_center()
            .gap(px(2.0))
            .track_scroll(&self.nav_scroll_handle)
            .children(snapshot.items.into_iter().map(|item| {
                self.render_nav_pill(item.icon_path, item.label, item.tab, active_tab.clone(), cx)
            }));

        let mut center = div().flex_1().min_w_0().overflow_hidden().relative();
        if let Some(slider) = slider {
            center = center.child(slider);
        }
        center = center.child(nav_scroll);

        div()
            .w_full()
            .border_b_1()
            .border_color(border_color)
            .py(px(4.0))
            .flex()
            .items_center()
            .child(left_arrow)
            .child(center)
            .child(right_arrow)
    }

    fn top_nav_snapshot(&self) -> TopNavSnapshot {
        let state = self.state.borrow();
        let session = &state.session;
        let custom_ids = session.provider_store.custom_provider_ids();
        let ordered_ids = session.settings.provider.ordered_provider_ids(&custom_ids);
        let mut nav_items: Vec<_> = ordered_ids
            .iter()
            .filter(|id| session.settings.provider.is_enabled(id))
            .filter_map(|id| {
                session
                    .provider_store
                    .providers
                    .iter()
                    .find(|p| p.provider_id == *id)
                    .map(|p| TopNavItem {
                        icon_path: p.icon_asset().to_string(),
                        label: p.display_name().to_string(),
                        tab: NavTab::Provider(id.clone()),
                    })
            })
            .collect();

        if session.settings.display.show_overview {
            nav_items.insert(
                0,
                TopNavItem {
                    icon_path: OVERVIEW_ICON.to_string(),
                    label: t!("nav.overview").to_string(),
                    tab: NavTab::Overview,
                },
            );
        }

        TopNavSnapshot {
            items: nav_items,
            previous_tab: session.nav.prev_active_tab.clone(),
            generation: session.nav.generation,
        }
    }

    fn measure_nav_scroll(
        handle: &ScrollHandle,
        active_index: Option<usize>,
        previous_index: Option<usize>,
    ) -> NavScrollMetrics {
        let offset = handle.offset();
        let max_offset = handle.max_offset();
        let threshold = px(SCROLL_THRESHOLD);
        let can_scroll_left = offset.x < threshold.negate();
        let can_scroll_right = max_offset.width > threshold && offset.x > max_offset.width.negate();

        let scroll_bounds = handle.bounds();
        let scroll_left = scroll_bounds.left();
        let scroll_offset_x = offset.x;

        let to_visual = |bounds: Bounds<Pixels>| NavSliderRect {
            left: bounds.left() - scroll_left + scroll_offset_x,
            width: bounds.size.width,
            height: bounds.size.height,
        };
        let target_rect = active_index
            .and_then(|ix| handle.bounds_for_item(ix))
            .map(to_visual);
        let from_rect = previous_index
            .and_then(|ix| handle.bounds_for_item(ix))
            .map(to_visual);
        let slider_layout = target_rect.map(|target| NavSliderLayout {
            target,
            from: from_rect,
        });

        NavScrollMetrics {
            can_scroll_left,
            can_scroll_right,
            left_target: Self::find_left_target(handle),
            right_target: Self::find_right_target(handle),
            slider_layout,
        }
    }

    fn render_nav_scroll_arrow(
        element_id: &'static str,
        icon_path: &'static str,
        enabled: bool,
        target: Option<usize>,
        scroll_handle: ScrollHandle,
        entity: Entity<Self>,
        indicator_color: Hsla,
    ) -> impl IntoElement {
        let mut arrow = div()
            .id(element_id)
            .w(px(INDICATOR_WIDTH))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer();

        if enabled {
            arrow = arrow
                .child(
                    svg()
                        .path(icon_path)
                        .size(px(10.0))
                        .text_color(indicator_color),
                )
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    if let Some(ix) = target {
                        scroll_handle.scroll_to_item(ix);
                    }
                    entity.update(cx, |_, cx| cx.notify());
                });
        }

        arrow
    }

    /// 渲染导航栏滑块背景（absolute 定位，带果冻动画）
    fn render_nav_slider(
        &self,
        layout: NavSliderLayout,
        generation: u64,
        bg: Hsla,
    ) -> impl IntoElement {
        let to_left = layout.target.left;
        let to_width = layout.target.width;
        let to_height = layout.target.height;

        // 如果有 from_rect 且和 target 不同 → 播放动画
        // 否则直接定位到 target（无动画）
        let animation_start = layout.from.filter(|from| {
            (from.left - to_left).abs() > px(1.0) || (from.width - to_width).abs() > px(1.0)
        });

        let base = div()
            .absolute()
            .top(px(0.0))
            .h(to_height)
            .rounded(px(8.0))
            .bg(bg);

        if let Some(from) = animation_start {
            let from_left = from.left;
            let from_width = from.width;
            base.with_animation(
                ElementId::Name(format!("nav-slider-{}", generation).into()),
                Animation::new(Duration::from_millis(SLIDER_ANIMATION_MS)),
                move |el: Div, delta| {
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
            .hover(|style: StyleRefinement| {
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
                    .font_weight(FontWeight::MEDIUM)
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
