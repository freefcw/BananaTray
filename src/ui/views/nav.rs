use crate::application::{provider_nav_alert_level, AppAction};
use crate::models::{NavTab, NavTabStyle, StatusLevel};
use crate::runtime;
use crate::theme::Theme;
use crate::ui::widgets::with_tooltip;
use crate::ui::AppView;
use gpui::{
    div, linear_color_stop, multi_stop_linear_gradient, point, px, relative, svg,
    transparent_black, Animation, AnimationExt, Context, Div, ElementId, FontWeight, Hsla,
    InteractiveElement, IntoElement, MouseButton, Negate, ParentElement, Pixels, ScrollHandle,
    Stateful, StatefulInteractiveElement, StyleRefinement, Styled, Window,
};
use rust_i18n::t;
use std::time::Duration;

/// Overview 导航图标路径（与 tray_settings.rs 保持一致）
const OVERVIEW_ICON: &str = "src/icons/overview.svg";
/// 导航项总数超过此阈值时启用紧凑模式（未选中仅显示图标，选中展开图标+文字）
const COMPACT_TAB_THRESHOLD: usize = 4;
/// 紧凑模式下未选中纯图标按钮的固定宽度
const COMPACT_ICON_PILL_WIDTH: f32 = 34.0;
/// 紧凑模式下未选中纯图标按钮的固定高度（与展开态胶囊高度一致）
const COMPACT_ICON_PILL_HEIGHT: f32 = 29.0;
/// 两侧指示器区域宽度（始终占位，避免可滚动态切换时布局跳动）
const INDICATOR_WIDTH: f32 = 18.0;
/// 渐变遮罩从内容边缘向内延伸的宽度
const FADE_WIDTH: f32 = 20.0;
/// 滚动容器的垂直内边距；absolute 定位的滑块 top 偏移需与它保持一致
const NAV_SCROLL_PADDING_Y: f32 = 4.0;
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
    alert_level: Option<StatusLevel>,
}

struct TopNavSnapshot {
    items: Vec<TopNavItem>,
    previous_tab: Option<NavTab>,
    generation: u64,
    /// 顶部导航栏展示风格（settings.display.nav_tab_style）
    tab_style: NavTabStyle,
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
pub(crate) struct NavSliderRect {
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
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.global::<Theme>();
        let border_color = theme.border.subtle;
        let slider_bg = theme.nav.pill_active_bg;
        let fade_from = theme.bg.panel;
        let fade_to = transparent_black();
        let snapshot = self.top_nav_snapshot();
        let active_index = snapshot.active_index(&active_tab);
        let previous_index = snapshot.previous_index();
        let item_count = snapshot.items.len();
        let compact_mode =
            snapshot.tab_style == NavTabStyle::Compact && item_count > COMPACT_TAB_THRESHOLD;

        let from_rect = self.sync_nav_transition(
            active_index,
            previous_index,
            item_count,
            snapshot.generation,
            window,
            cx,
        );

        let metrics = Self::measure_nav_scroll(&self.nav_scroll_handle, active_index, from_rect);
        let has_slider = metrics.slider_layout.is_some();

        let left_arrow = self.render_nav_scroll_arrow(
            true,
            metrics.can_scroll_left,
            metrics.left_target,
            item_count,
            cx,
        );
        let right_arrow = self.render_nav_scroll_arrow(
            false,
            metrics.can_scroll_right,
            metrics.right_target,
            item_count,
            cx,
        );
        let slider = metrics
            .slider_layout
            .map(|layout| self.render_nav_slider(layout, snapshot.generation, slider_bg));

        let pill_gap = if compact_mode { px(4.0) } else { px(2.0) };
        let nav_scroll = div()
            .id("nav-provider-scroll")
            .overflow_x_scroll()
            .scrollbar_width(px(0.0))
            .flex()
            .items_center()
            .gap(pill_gap)
            .track_scroll(&self.nav_scroll_handle)
            .children(snapshot.items.into_iter().enumerate().map(|(ix, item)| {
                self.render_nav_pill(ix, item, active_tab.clone(), compact_mode, has_slider, cx)
            }));

        let mut center = div()
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .relative()
            .py(px(NAV_SCROLL_PADDING_Y));
        if let Some(slider) = slider {
            center = center.child(slider);
        }
        center = center.child(nav_scroll);

        // 左右边缘渐变遮罩：可滚动时盖在内容截断处，缓和硬裁剪边缘
        if metrics.can_scroll_left {
            center = center.child(Self::render_nav_fade(true, fade_from, fade_to));
        }
        if metrics.can_scroll_right {
            center = center.child(Self::render_nav_fade(false, fade_from, fade_to));
        }

        div()
            .w_full()
            .border_b_1()
            .border_color(border_color)
            .px(px(2.0))
            .gap(px(2.0))
            .flex()
            .items_center()
            .child(left_arrow)
            .child(center)
            .child(right_arrow)
    }

    fn top_nav_snapshot(&self) -> TopNavSnapshot {
        let state = self.state.borrow();
        let session = &state.session;
        let show_status_dots = session.settings.display.nav_status_dots;
        let mut nav_items: Vec<_> = session
            .provider_store
            .enabled_providers(&session.settings)
            .map(|provider| TopNavItem {
                icon_path: provider.icon_asset().to_string(),
                label: provider.display_name().to_string(),
                tab: NavTab::Provider(provider.provider_id.clone()),
                alert_level: if show_status_dots {
                    provider_nav_alert_level(&session.settings, provider)
                } else {
                    None
                },
            })
            .collect();

        if session.settings.display.show_overview {
            nav_items.insert(
                0,
                TopNavItem {
                    icon_path: OVERVIEW_ICON.to_string(),
                    label: t!("nav.overview").to_string(),
                    tab: NavTab::Overview,
                    alert_level: None,
                },
            );
        }

        TopNavSnapshot {
            items: nav_items,
            previous_tab: session.nav.prev_active_tab.clone(),
            generation: session.nav.generation,
            tab_style: session.settings.display.nav_tab_style,
        }
    }

    /// 同步 Tab 切换时的动画起点锁定与下一帧滚动归位。
    ///
    /// 紧凑模式下切换 Tab 会改变旧项（收起文字）与新项（展开文字）的自身宽度，
    /// 因此在切换首帧先锁定旧项展开态的 `from_rect` 并触发下一帧重绘；
    /// 第 2 帧拿到新项展开后的真实 `bounds_for_item` 后再滚入视口并启动滑块动画。
    fn sync_nav_transition(
        &self,
        active_index: Option<usize>,
        previous_index: Option<usize>,
        item_count: usize,
        generation: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<NavSliderRect> {
        // 首帧进入新的 generation：锁定切换前旧选中项（仍处于展开态）的视觉位置
        if self.nav_anim_from.get().map(|(g, _)| g) != Some(generation) {
            let from_rect = previous_index
                .and_then(|ix| Self::visual_rect_of_item(&self.nav_scroll_handle, ix));
            self.nav_anim_from.set(Some((generation, from_rect)));
            let entity = cx.entity().clone();
            window.on_next_frame(move |_, cx| {
                entity.update(cx, |_, cx| cx.notify());
            });
            return from_rect;
        }

        let cached_from = self.nav_anim_from.get().and_then(|(_, rect)| rect);

        if self.nav_settled_generation.get() != Some(generation) {
            match active_index {
                Some(ix) => {
                    if Self::scroll_nav_item_into_view(&self.nav_scroll_handle, ix, item_count) {
                        self.nav_settled_generation.set(Some(generation));
                    } else {
                        let entity = cx.entity().clone();
                        window.on_next_frame(move |_, cx| {
                            entity.update(cx, |_, cx| cx.notify());
                        });
                    }
                }
                None => {
                    self.nav_settled_generation.set(Some(generation));
                }
            }
        }

        cached_from
    }

    /// 将指定索引的导航项滚入可视区域，返回是否已完成布局测量
    fn scroll_nav_item_into_view(handle: &ScrollHandle, ix: usize, item_count: usize) -> bool {
        let Some(cb) = handle.bounds_for_item(ix) else {
            return false;
        };
        let bounds = handle.bounds();
        if bounds.size.width <= px(0.0) {
            return false;
        }
        let max_scroll = handle.max_offset().width.max(px(0.0));
        if max_scroll <= px(SCROLL_THRESHOLD) || ix == 0 {
            handle.set_offset(point(px(0.0), px(0.0)));
            return true;
        }
        if ix + 1 >= item_count {
            handle.set_offset(point(max_scroll.negate(), px(0.0)));
            return true;
        }

        let offset_x = handle.offset().x;
        let view_left = bounds.left();
        let view_right = bounds.right();
        let vis_left = cb.left() + offset_x;
        let vis_right = cb.right() + offset_x;

        let new_offset_x = if vis_left < view_left {
            (view_left - cb.left()).clamp(max_scroll.negate(), px(0.0))
        } else if vis_right > view_right {
            (view_right - cb.right()).clamp(max_scroll.negate(), px(0.0))
        } else {
            offset_x
        };

        if (new_offset_x - offset_x).abs() > px(0.5) {
            handle.set_offset(point(new_offset_x, px(0.0)));
        }
        true
    }

    /// 将 item 的 layout bounds 换算成视觉坐标（扣除滚动偏移）。
    /// 首帧 prepaint 未完成测量（bounds 为空）时返回 None。
    fn visual_rect_of_item(handle: &ScrollHandle, ix: usize) -> Option<NavSliderRect> {
        let bounds = handle.bounds_for_item(ix)?;
        let scroll_bounds = handle.bounds();
        if scroll_bounds.size.width <= px(0.0) {
            return None;
        }
        let offset_x = handle.offset().x;
        Some(NavSliderRect {
            left: bounds.left() - scroll_bounds.left() + offset_x,
            width: bounds.size.width,
            height: bounds.size.height,
        })
    }

    fn measure_nav_scroll(
        handle: &ScrollHandle,
        active_index: Option<usize>,
        from_rect: Option<NavSliderRect>,
    ) -> NavScrollMetrics {
        let offset = handle.offset();
        let max_offset = handle.max_offset();
        let threshold = px(SCROLL_THRESHOLD);
        let can_scroll_left = offset.x < threshold.negate();
        let can_scroll_right = max_offset.width > threshold && offset.x > max_offset.width.negate();

        let target_rect = active_index.and_then(|ix| Self::visual_rect_of_item(handle, ix));
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
        &self,
        is_left: bool,
        enabled: bool,
        target: Option<usize>,
        item_count: usize,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let (element_id, icon_path) = if is_left {
            ("nav-arrow-left", "src/icons/chevron-left.svg")
        } else {
            ("nav-arrow-right", "src/icons/chevron-right.svg")
        };

        let mut arrow = div()
            .id(element_id)
            .w(px(INDICATOR_WIDTH))
            .h_full()
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center();

        if enabled {
            let theme = cx.global::<Theme>();
            let indicator_color = theme.text.muted;
            let hover_bg = theme.bg.subtle;
            let scroll_handle = self.nav_scroll_handle.clone();
            let entity = cx.entity().clone();

            arrow = arrow
                .cursor_pointer()
                .hover(move |style| style.bg(hover_bg))
                .child(
                    svg()
                        .path(icon_path)
                        .size(px(11.0))
                        .text_color(indicator_color),
                )
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    cx.stop_propagation();
                    if let Some(ix) = target {
                        Self::scroll_nav_item_into_view(&scroll_handle, ix, item_count);
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
            // 抵消 scroll 容器自身的垂直 padding，让滑块与 pill 垂直对齐
            .top(px(NAV_SCROLL_PADDING_Y))
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

    /// 滚动边缘渐变遮罩（面板底色 → 透明），仅在对应方向可滚动时叠加。
    /// 纯装饰层不拦截点击，被遮罩半露的 Tab 仍可点按。
    fn render_nav_fade(is_left: bool, fade_from: Hsla, fade_to: Hsla) -> Div {
        let base = div()
            .absolute()
            .top(px(0.0))
            .bottom(px(0.0))
            .w(px(FADE_WIDTH));
        if is_left {
            base.left(px(0.0)).bg(multi_stop_linear_gradient(
                90.,
                &[
                    linear_color_stop(fade_from, 0.),
                    linear_color_stop(fade_to, 1.),
                ],
            ))
        } else {
            base.right(px(0.0)).bg(multi_stop_linear_gradient(
                270.,
                &[
                    linear_color_stop(fade_from, 0.),
                    linear_color_stop(fade_to, 1.),
                ],
            ))
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
        count.checked_sub(1)
    }

    /// 渲染单个导航项：
    /// - 常规模式（`<= 4` 项或 Full 风格）以及当前选中项：展示 `图标 + 名称` 胶囊
    /// - Compact 风格且项数较多时的未选中项：仅展示居中图标，悬停显示名称 Tooltip
    /// - `nav_status_dots` 开启时，配额异常的 Provider 在图标右上角叠加状态色点
    fn render_nav_pill(
        &self,
        ix: usize,
        item: TopNavItem,
        active_tab: NavTab,
        compact_mode: bool,
        has_slider: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let tab = item.tab;
        let is_active = tab == active_tab;
        let icon_only = compact_mode && !is_active;
        let theme = cx.global::<Theme>().clone();
        let entity = cx.entity().clone();

        let (text_color, icon_color) = if is_active {
            (theme.nav.pill_active_text, theme.nav.pill_active_text)
        } else {
            (theme.text.muted, theme.text.muted)
        };
        // 字母徽章的镂空色 = 图标所在背景：选中项是胶囊底色，未选中是面板底色
        let icon_surface = if is_active {
            theme.nav.pill_active_bg
        } else {
            theme.bg.panel
        };

        let hover_bg = theme.bg.subtle;
        let pill_id = ElementId::Name(format!("nav-pill-{ix}").into());

        // 状态点描边跟随点背后的底色：选中项是滑块底色，未选中项是面板底色
        let mut icon_wrap =
            div()
                .relative()
                .flex_shrink_0()
                .child(crate::ui::widgets::render_provider_icon(
                    item.icon_path,
                    px(15.0),
                    icon_color,
                    icon_surface,
                ));
        if let Some(level) = item.alert_level {
            let dot_color = theme.status_badge_color(level);
            let ring_color = if is_active {
                theme.nav.pill_active_bg
            } else {
                theme.bg.panel
            };
            icon_wrap = icon_wrap.child(
                div()
                    .absolute()
                    .top(px(-2.0))
                    .right(px(-2.0))
                    .w(px(6.0))
                    .h(px(6.0))
                    .rounded_full()
                    .bg(dot_color)
                    .border_1()
                    .border_color(ring_color),
            );
        }

        // 两种形态共享的胶囊外壳：flex + 圆角 + 指针 + 非激活态悬停底色
        let pill_shell = div()
            .flex_shrink_0()
            .flex()
            .items_center()
            .rounded(px(8.0))
            .cursor_pointer()
            .hover(
                move |style: StyleRefinement| {
                    if is_active {
                        style
                    } else {
                        style.bg(hover_bg)
                    }
                },
            );

        let stateful_pill = if icon_only {
            let base = pill_shell
                .w(px(COMPACT_ICON_PILL_WIDTH))
                .h(px(COMPACT_ICON_PILL_HEIGHT))
                .justify_center()
                .child(icon_wrap);
            with_tooltip(pill_id, &item.label, &theme, base)
        } else {
            let mut base = pill_shell.id(pill_id).gap(px(5.0)).px(px(10.0)).py(px(6.0));

            if is_active && !has_slider {
                base = base.bg(theme.nav.pill_active_bg);
            }

            base.child(icon_wrap).child(
                div()
                    .text_size(px(13.0))
                    .line_height(relative(1.3))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(text_color)
                    .whitespace_nowrap()
                    .child(item.label),
            )
        };

        stateful_pill.on_mouse_down(MouseButton::Left, move |_, _, cx| {
            entity.update(cx, |view, cx| {
                runtime::dispatch_in_context(&view.state, AppAction::SelectNavTab(tab.clone()), cx);
            });
        })
    }
}
