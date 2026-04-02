use super::AppView;
use crate::models::NavTab;
use crate::theme::Theme;
use gpui::prelude::FluentBuilder as _;
use gpui::*;

/// 两侧指示器区域宽度（像素），与原来的 px(14) padding 一致
const INDICATOR_WIDTH: f32 = 14.0;
/// 渐变遮罩从内容边缘向内延伸的宽度
const FADE_WIDTH: f32 = 20.0;
/// 滚动偏移量超过此阈值才显示指示器（避免微小偏移时闪烁）
const SCROLL_THRESHOLD: f32 = 2.0;

impl AppView {
    pub(crate) fn render_top_nav(
        &self,
        active_tab: NavTab,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.global::<Theme>();
        let state_ref = self.state.borrow();
        let settings = state_ref.settings.clone();
        let providers = state_ref.provider_store.providers.clone();
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
        let panel_bg = theme.bg_panel;

        // 读取当前滚动状态
        let offset = self.nav_scroll_handle.offset();
        let max_offset = self.nav_scroll_handle.max_offset();

        let threshold = px(SCROLL_THRESHOLD);
        let can_scroll_left = offset.x < threshold.negate();
        let can_scroll_right = max_offset.width > threshold && offset.x > max_offset.width.negate();

        // 箭头颜色
        let indicator_color = theme.text_muted;
        let fade_from = panel_bg;
        let fade_to: Hsla = transparent_black();

        // 整体布局：三栏 [左指示器] [滚动内容] [右指示器]
        div()
            .w_full()
            .border_b_1()
            .border_color(border_color)
            .py(px(4.0))
            .flex()
            .items_center()
            .child(
                // ── 左侧指示器区域 ──
                div()
                    .w(px(INDICATOR_WIDTH))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(can_scroll_left, |el| {
                        el.child(
                            svg()
                                .path("src/icons/chevron-left.svg")
                                .size(px(10.0))
                                .text_color(indicator_color),
                        )
                    }),
            )
            .child(
                // ── 中间滚动区域 ──
                // 外层 wrapper: relative + overflow_hidden，承载渐变遮罩
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .relative()
                    .child(
                        // 实际滚动容器
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
                    )
                    // 左侧渐变遮罩 —— 盖在滚动内容边缘上
                    .when(can_scroll_left, |el| {
                        el.child(
                            div()
                                .absolute()
                                .top(px(0.0))
                                .left(px(0.0))
                                .bottom(px(0.0))
                                .w(px(FADE_WIDTH))
                                .bg(multi_stop_linear_gradient(
                                    90.,
                                    &[
                                        linear_color_stop(fade_from, 0.),
                                        linear_color_stop(fade_to, 1.),
                                    ],
                                )),
                        )
                    })
                    // 右侧渐变遮罩
                    .when(can_scroll_right, |el| {
                        el.child(
                            div()
                                .absolute()
                                .top(px(0.0))
                                .right(px(0.0))
                                .bottom(px(0.0))
                                .w(px(FADE_WIDTH))
                                .bg(multi_stop_linear_gradient(
                                    270.,
                                    &[
                                        linear_color_stop(fade_from, 0.),
                                        linear_color_stop(fade_to, 1.),
                                    ],
                                )),
                        )
                    }),
            )
            .child(
                // ── 右侧指示器区域 ──
                div()
                    .w(px(INDICATOR_WIDTH))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(can_scroll_right, |el| {
                        el.child(
                            svg()
                                .path("src/icons/chevron-right.svg")
                                .size(px(10.0))
                                .text_color(indicator_color),
                        )
                    }),
            )
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
        let state = self.state.clone();
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
            theme.border_strong
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
                state.borrow_mut().nav.switch_to(tab);
                entity.update(cx, |_, cx| {
                    cx.notify();
                });
            })
    }
}
