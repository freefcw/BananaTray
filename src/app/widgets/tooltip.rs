#![allow(dead_code)]

use crate::theme::Theme;
use gpui::*;

/// A simple tooltip view rendered by GPUI's native tooltip system.
/// Appears at the mouse position, automatically adjusts direction to
/// stay within window bounds, and paints above all other elements.
pub(crate) struct TooltipView {
    text: SharedString,
    bg: Hsla,
    border: Hsla,
    text_color: Hsla,
}

impl Render for TooltipView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(8.0))
            .py(px(4.0))
            .rounded(px(6.0))
            .bg(self.bg)
            .border_1()
            .border_color(self.border)
            .shadow_md()
            .text_size(px(11.0))
            .text_color(self.text_color)
            .whitespace_nowrap()
            .child(self.text.clone())
    }
}

/// Attach a native GPUI tooltip to a stateful div.
/// The tooltip is rendered at the topmost layer, never clipped by parent containers,
/// and automatically repositions to stay within window bounds.
pub(crate) fn with_tooltip(
    id: impl Into<ElementId>,
    tooltip_text: &str,
    theme: &Theme,
    child: Div,
) -> Stateful<Div> {
    let text: SharedString = tooltip_text.to_string().into();
    let bg = theme.bg_panel;
    let border = theme.border_subtle;
    let text_color = theme.text_primary;

    child.id(id).tooltip(move |_window, cx| {
        cx.new(|_cx| TooltipView {
            text: text.clone(),
            bg,
            border,
            text_color,
        })
        .into()
    })
}
