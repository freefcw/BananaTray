use super::{persist_settings, AppView};
use crate::theme::Theme;
use gpui::*;

const AUTO_HIDE_ICON: &str = "src/icons/display.svg";

impl AppView {
    pub(crate) fn render_settings_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let settings = self.state.borrow().settings.clone();
        let theme = cx.global::<Theme>();
        let state = self.state.clone();
        let entity = cx.entity().clone();
        let auto_hide_state = state.clone();
        let auto_hide_entity = entity.clone();

        div()
            .px(px(12.0))
            .py(px(12.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .rounded(px(14.0))
                    .bg(theme.bg_card)
                    .border_1()
                    .border_color(theme.border_subtle)
                    .px(px(14.0))
                    .py(px(12.0))
                    .cursor_pointer()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.0))
                            .child(super::widgets::render_footer_glyph(AUTO_HIDE_ICON, theme))
                            .child(
                                div()
                                    .flex_col()
                                    .gap(px(3.0))
                                    .child(
                                        div()
                                            .text_size(px(14.0))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(theme.text_primary)
                                            .child("Auto-hide window"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .text_color(theme.text_secondary)
                                            .child(
                                                "Close the tray popover when focus leaves the app.",
                                            ),
                                    ),
                            ),
                    )
                    .child(self.render_toggle_switch_small(settings.auto_hide_window, theme))
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        let settings = {
                            let mut app_state = auto_hide_state.borrow_mut();
                            app_state.settings.auto_hide_window =
                                !app_state.settings.auto_hide_window;
                            app_state.settings.clone()
                        };
                        persist_settings(&settings);
                        auto_hide_entity.update(cx, |_, cx| {
                            cx.notify();
                        });
                    }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .rounded(px(14.0))
                    .bg(theme.bg_card)
                    .border_1()
                    .border_color(theme.border_subtle)
                    .px(px(14.0))
                    .py(px(12.0))
                    .child(
                        div()
                            .flex_col()
                            .gap(px(3.0))
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme.text_primary)
                                    .child("Visible providers"),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(theme.text_secondary)
                                    .child(
                                    "Show only the providers you care about in the tray header.",
                                ),
                            ),
                    )
                    .child(div().flex().gap(px(6.0)).children((3..=5).map(|count| {
                        let state = state.clone();
                        let entity = entity.clone();
                        let is_active = settings.visible_provider_count == count;
                        div()
                            .min_w(px(28.0))
                            .px(px(8.0))
                            .py(px(5.0))
                            .rounded_full()
                            .bg(if is_active {
                                theme.element_selected
                            } else {
                                theme.bg_subtle
                            })
                            .border_1()
                            .border_color(theme.border_subtle)
                            .text_size(px(11.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(if is_active {
                                theme.element_active
                            } else {
                                theme.text_primary
                            })
                            .cursor_pointer()
                            .child(count.to_string())
                            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                let settings = {
                                    let mut app_state = state.borrow_mut();
                                    app_state.settings.visible_provider_count = count;
                                    app_state.settings.clone()
                                };
                                persist_settings(&settings);
                                entity.update(cx, |_, cx| {
                                    cx.notify();
                                });
                            })
                    }))),
            )
            .into_any_element()
    }
}
