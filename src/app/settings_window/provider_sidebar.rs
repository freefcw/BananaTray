use super::SettingsView;
use crate::app::persist_settings;
use crate::app::widgets::render_card_separator;
use crate::models::{AppSettings, ProviderKind};
use crate::theme::Theme;
use gpui::*;

impl SettingsView {
    /// Render Providers settings tab — two-column layout
    pub(super) fn render_providers_tab(
        &self,
        settings: &AppSettings,
        theme: &Theme,
        viewport: Size<Pixels>,
    ) -> Div {
        let selected = self.state.borrow().settings_ui.selected_provider;
        let providers = self.state.borrow().provider_store.providers.clone();

        div()
            .flex()
            .h_full()
            .overflow_hidden()
            .child(self.render_provider_sidebar(&providers, selected, settings, theme, viewport))
            .child(
                self.render_provider_detail_panel(&providers, selected, settings, theme, viewport),
            )
    }

    // ══════ Left sidebar ══════

    fn render_provider_sidebar(
        &self,
        _providers: &[crate::models::ProviderStatus],
        selected: ProviderKind,
        settings: &AppSettings,
        theme: &Theme,
        viewport: Size<Pixels>,
    ) -> Div {
        // NOTE: 不使用 render_card()，因为它带有 overflow_hidden()，
        // 会让 Taffy 将 card 的 min-height 设为 0，导致 card 在 Scrollable
        // 内部被压缩到容器高度，永远不会溢出，滚动条无法触发。
        let mut card = div()
            .flex_col()
            .rounded(px(10.0))
            .bg(rgb(0xffffff))
            .py(px(4.0));
        let ordered = settings.ordered_providers();

        for (i, kind) in ordered.iter().enumerate() {
            let is_selected = *kind == selected;
            let is_enabled = settings.is_provider_enabled(*kind);

            let status = _providers.iter().find(|p| p.kind == *kind);
            let icon = status
                .map(|p| p.icon_asset().to_string())
                .unwrap_or_else(|| "src/icons/provider-unknown.svg".to_string());
            let display_name = status
                .map(|p| p.display_name().to_string())
                .unwrap_or_else(|| format!("{:?}", kind));

            let state = self.state.clone();
            let kind_copy = *kind;

            if i > 0 {
                card = card.child(render_card_separator());
            }

            let mut item = div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .px(px(10.0))
                .py(px(8.0))
                .cursor_pointer();

            if is_selected {
                item = item.mx(px(4.0)).rounded(px(8.0)).bg(theme.element_selected);
            }

            item =
                item
                    // Provider icon
                    .child(svg().path(icon).size(px(22.0)).flex_shrink_0().text_color(
                        if is_selected {
                            theme.element_active
                        } else {
                            theme.text_secondary
                        },
                    ))
                    // Name + green dot (enabled indicator)
                    .child({
                        let name_row = div().flex().items_center().gap(px(4.0)).flex_1().child(
                            div()
                                .text_size(px(12.5))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(if is_selected {
                                    theme.element_active
                                } else {
                                    theme.text_primary
                                })
                                .child(display_name),
                        );
                        if is_enabled {
                            name_row.child(
                                div()
                                    .w(px(6.0))
                                    .h(px(6.0))
                                    .rounded_full()
                                    .bg(theme.status_success),
                            )
                        } else {
                            name_row
                        }
                    });

            // Reorder arrows — always reserve space, only interactive when selected
            {
                let is_first = i == 0;
                let is_last = i == ordered.len() - 1;
                let state_up = self.state.clone();
                let state_down = self.state.clone();
                let kind_up = *kind;
                let kind_down = *kind;

                let mut arrows = div().flex_col().flex_shrink_0();

                let mut up_btn = div()
                    .w(px(16.0))
                    .h(px(12.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(3.0))
                    .text_size(px(8.0));

                let mut down_btn = div()
                    .w(px(16.0))
                    .h(px(12.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(3.0))
                    .text_size(px(8.0));

                if !is_selected {
                    // Invisible placeholder to keep height stable
                    up_btn = up_btn.text_color(transparent_black()).child("▲");
                    down_btn = down_btn.text_color(transparent_black()).child("▼");
                } else {
                    if is_first {
                        up_btn = up_btn.text_color(theme.border_subtle).child("▲");
                    } else {
                        up_btn = up_btn
                            .cursor_pointer()
                            .text_color(theme.element_active)
                            .hover(|s| s.bg(theme.border_subtle))
                            .child("▲")
                            .on_mouse_down(MouseButton::Left, move |_, window, _| {
                                let mut s = state_up.borrow_mut();
                                if s.settings.move_provider_up(kind_up) {
                                    persist_settings(&s.settings);
                                }
                                drop(s);
                                window.refresh();
                            });
                    }

                    if is_last {
                        down_btn = down_btn.text_color(theme.border_subtle).child("▼");
                    } else {
                        down_btn = down_btn
                            .cursor_pointer()
                            .text_color(theme.element_active)
                            .hover(|s| s.bg(theme.border_subtle))
                            .child("▼")
                            .on_mouse_down(MouseButton::Left, move |_, window, _| {
                                let mut s = state_down.borrow_mut();
                                if s.settings.move_provider_down(kind_down) {
                                    persist_settings(&s.settings);
                                }
                                drop(s);
                                window.refresh();
                            });
                    }
                }

                arrows = arrows.child(up_btn).child(down_btn);
                item = item.child(arrows);
            }

            item = item.on_mouse_down(MouseButton::Left, move |_, window, _| {
                state.borrow_mut().settings_ui.selected_provider = kind_copy;
                window.refresh();
            });

            card = card.child(item);
        }

        // Tab bar ≈ 65px, sidebar top-padding = 8px
        let sidebar_scroll_h = viewport.height - px(65.0) - px(8.0);

        div()
            .flex_col()
            .flex_none()
            .flex_basis(px(190.0))
            .pl(px(8.0))
            .pr(px(4.0))
            .pt(px(8.0))
            .overflow_hidden()
            .child(
                div()
                    .id("provider-sidebar-scroll")
                    .flex_col()
                    .h(sidebar_scroll_h)
                    .overflow_y_scroll()
                    .child(card),
            )
    }
}
