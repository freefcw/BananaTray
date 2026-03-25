use super::AppView;
use crate::theme::Theme;
use gpui::*;

impl AppView {
    pub(crate) fn render_footer_glyph(
        &self,
        icon_path: &'static str,
        theme: &Theme,
    ) -> impl IntoElement {
        div()
            .w(px(18.0))
            .h(px(18.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.0))
            .border_1()
            .border_color(theme.text_accent_soft)
            .bg(theme.bg_subtle)
            .child(self.render_svg_icon(icon_path, px(11.0), theme.text_accent))
    }

    pub(crate) fn render_svg_icon(
        &self,
        path: &'static str,
        size: Pixels,
        color: Hsla,
    ) -> impl IntoElement {
        svg().path(path).size(size).text_color(color)
    }

    pub(crate) fn render_toggle_switch(&self, enabled: bool, theme: &Theme) -> impl IntoElement {
        div()
            .w(px(36.0))
            .h(px(20.0))
            .flex()
            .items_center()
            .rounded_full()
            .px(px(2.0))
            .bg(if enabled {
                theme.element_selected
            } else {
                theme.bg_subtle
            })
            .border_1()
            .border_color(if enabled {
                theme.text_accent_soft
            } else {
                theme.border_strong
            })
            .child(
                div()
                    .w(px(14.0))
                    .h(px(14.0))
                    .rounded_full()
                    .bg(theme.element_active)
                    .ml(if enabled { px(16.0) } else { px(0.0) }),
            )
    }
}
