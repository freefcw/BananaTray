use crate::theme::Theme;
use gpui::{
    div, px, AnyElement, App, ElementId, Hsla, InteractiveElement, IntoElement, MouseButton,
    MouseDownEvent, ParentElement, Styled, Window,
};

use super::icon::render_svg_icon;
use super::tooltip::with_tooltip;

pub(crate) struct IconTooltipButtonOptions {
    pub tooltip_text: Option<String>,
    pub enabled: bool,
    pub icon_color: Hsla,
    pub disabled_icon_color: Hsla,
    pub hover_bg: Hsla,
}

/// 可复用的图标按钮：支持 hover 背景、tooltip、禁用态和点击回调。
pub(crate) fn render_icon_tooltip_button<F>(
    id: ElementId,
    icon: &'static str,
    options: IconTooltipButtonOptions,
    theme: &Theme,
    on_click: F,
) -> AnyElement
where
    F: Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
{
    let IconTooltipButtonOptions {
        tooltip_text,
        enabled,
        icon_color,
        disabled_icon_color,
        hover_bg,
    } = options;

    let base = div()
        .w(px(32.0))
        .h(px(32.0))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(10.0))
        .child(render_svg_icon(
            icon,
            px(16.0),
            if enabled {
                icon_color
            } else {
                disabled_icon_color
            },
        ));

    if !enabled {
        return base.into_any_element();
    }

    let interactive = base
        .cursor_pointer()
        .hover(move |style| style.bg(hover_bg))
        .on_mouse_down(MouseButton::Left, on_click);

    if let Some(tooltip_text) = tooltip_text {
        with_tooltip(id, &tooltip_text, theme, interactive).into_any_element()
    } else {
        interactive.into_any_element()
    }
}
