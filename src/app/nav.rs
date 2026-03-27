use super::AppView;
use crate::models::NavTab;
use crate::theme::Theme;
use gpui::*;

impl AppView {
    pub(crate) fn render_top_nav(
        &self,
        active_tab: NavTab,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.global::<Theme>();
        let state_ref = self.state.borrow();
        let visible_provider_count = state_ref.settings.visible_provider_count.clamp(3, 6);
        let settings = state_ref.settings.clone();
        drop(state_ref);

        let provider_order = settings.ordered_providers();
        // 仅显示已启用的 Provider tab
        let nav_items: Vec<_> = provider_order
            .into_iter()
            .filter(|kind| settings.is_provider_enabled(*kind))
            .take(visible_provider_count)
            .map(|kind| {
                (
                    kind.icon_asset(),
                    kind.display_name(),
                    NavTab::Provider(kind),
                )
            })
            .collect();

        div()
            .w_full()
            .border_b_1()
            .border_color(theme.border_subtle)
            .px(px(8.0))
            .pt(px(2.0)) // 减小上边距
            .pb(px(0.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    // 不需要背景色了，直接放在透明背景上
                    .children(nav_items.into_iter().map(|(icon, label, tab)| {
                        self.render_nav_item(icon, label, tab, active_tab, cx)
                    })),
            )
    }

    fn render_nav_item(
        &self,
        icon_path: &'static str,
        label: &'static str,
        tab: NavTab,
        active_tab: NavTab,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let is_active = tab == active_tab;
        let theme = cx.global::<Theme>();
        let state = self.state.clone();
        let entity = cx.entity().clone();

        let (icon_color, text_color) = if is_active {
            (theme.text_primary, theme.text_primary)
        } else {
            (theme.text_muted, theme.text_muted)
        };

        let item =
            div()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(2.0))
                .pt(px(4.0))
                .pb(px(4.0))
                .px(px(8.0)) // 固定水平间距，使其变为小巧的胶囊
                .rounded(px(6.0))
                .mt(px(2.0))
                .mb(px(2.0))
                .cursor_pointer()
                .bg(if is_active {
                    theme.bg_subtle // 仅仅是底色加深一点点
                } else {
                    transparent_black()
                })
                .hover(|style| style.bg(theme.border_subtle))
                .child(div().flex().w_full().justify_center().child(
                    super::widgets::render_svg_icon(icon_path, px(16.0), icon_color),
                ))
                .child(
                    div()
                        .text_size(px(11.0))
                        .font_weight(if is_active {
                            FontWeight::BOLD
                        } else {
                            FontWeight::MEDIUM
                        })
                        .text_color(text_color)
                        .child(label),
                );

        item.on_mouse_down(MouseButton::Left, move |_, _, cx| {
            let mut app_state = state.borrow_mut();
            app_state.active_tab = tab;
            if let NavTab::Provider(kind) = tab {
                app_state.last_provider_kind = kind;
            }
            entity.update(cx, |_, cx| {
                cx.notify();
            });
        })
    }
}
