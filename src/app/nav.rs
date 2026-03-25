use super::settings_window::schedule_open_settings_window;
use super::AppView;
use crate::models::{NavTab, ProviderKind};
use crate::theme::Theme;
use gpui::*;
use log::info;

const SETTINGS_ICON: &str = "src/icons/settings.svg";

impl AppView {
    pub(crate) fn render_top_nav(
        &self,
        active_tab: NavTab,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let settings_action = self.render_settings_trigger(cx);
        let theme = cx.global::<Theme>();
        let visible_provider_count = self
            .state
            .borrow()
            .settings
            .visible_provider_count
            .clamp(3, 5);
        let provider_order = [
            ProviderKind::Claude,
            ProviderKind::Gemini,
            ProviderKind::Copilot,
            ProviderKind::Amp,
            ProviderKind::Kimi,
            ProviderKind::Codex,
        ];
        let nav_items: Vec<_> = provider_order
            .into_iter()
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
            .flex_col()
            .w_full()
            .border_b_1()
            .border_color(theme.border_subtle)
            .px(px(10.0))
            .pt(px(8.0))
            .pb(px(6.0))
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.text_primary)
                            .child("BananaTray"),
                    )
                    .child(settings_action),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(1.0))
                    .rounded(px(8.0))
                    .bg(theme.bg_subtle)
                    .p(px(2.0))
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

        let item = div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .gap(px(5.0))
            .py(px(5.0))
            .rounded(px(6.0))
            .cursor_pointer()
            .bg(if is_active {
                theme.bg_card
            } else {
                transparent_black()
            })
            .child(
                svg()
                    .path(icon_path)
                    .size(px(13.0))
                    .text_color(if is_active {
                        theme.text_accent
                    } else {
                        theme.text_muted
                    }),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .font_weight(if is_active {
                        FontWeight::SEMIBOLD
                    } else {
                        FontWeight::MEDIUM
                    })
                    .text_color(if is_active {
                        theme.text_primary
                    } else {
                        theme.text_muted
                    })
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

    fn render_settings_trigger(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<Theme>();
        let state = self.state.clone();

        div()
            .w(px(28.0))
            .h(px(28.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(8.0))
            .cursor_pointer()
            .bg(theme.bg_subtle)
            .border_1()
            .border_color(theme.border_subtle)
            .child(self.render_svg_icon(SETTINGS_ICON, px(13.0), theme.text_secondary))
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                info!(target: "settings", "settings trigger clicked from tray header");
                window.remove_window();
                let settings_state = state.clone();
                schedule_open_settings_window(settings_state, cx);
            })
    }
}
