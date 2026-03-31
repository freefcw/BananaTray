use super::{persist_settings, AppView};
use crate::theme::Theme;
use gpui::*;
use rust_i18n::t;

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
                                            .child(t!("settings.auto_hide").to_string()),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .text_color(theme.text_secondary)
                                            .child(t!("settings.auto_hide.desc").to_string()),
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
            .into_any_element()
    }
}
