//! 限流重置卡。
//!
//! 不复用额度卡：没有大数字、百分号、进度条和 OK/WARN/LOW 徽章。
//! 每一条只放重置类型、获取时间和到期时间。行高跟 `PopupLayout` 的重置常量对齐。

use crate::application::RateLimitResetSectionView;
use crate::models::PopupLayout;
use crate::theme::Theme;
use gpui::{
    div, px, relative, Div, ElementId, FontWeight, InteractiveElement, ParentElement, Stateful,
    Styled,
};
use rust_i18n::t;

pub(crate) fn render_rate_limit_resets(
    section: &RateLimitResetSectionView,
    theme: &Theme,
) -> Stateful<Div> {
    let mut card = div()
        .id(ElementId::Name("rate-limit-resets".into()))
        .w_full()
        .flex_col()
        .gap(px(PopupLayout::RESET_CARD_GAP))
        .px(px(16.0))
        .py(px(PopupLayout::RESET_CARD_PAD_Y / 2.0))
        .rounded(px(12.0))
        .bg(theme.bg.card_inner)
        .border_1()
        .border_color(theme.border.subtle)
        .child(reset_line(
            &t!("codex.reset.heading").to_uppercase(),
            theme.text.muted,
            FontWeight::SEMIBOLD,
        ));

    for row in &section.rows {
        card = card.child(render_reset_row(row, theme));
    }
    if let Some(summary) = &section.summary {
        card = card.child(reset_line(
            summary,
            theme.text.secondary,
            FontWeight::NORMAL,
        ));
    }
    card
}

fn render_reset_row(row: &crate::application::RateLimitResetRowView, theme: &Theme) -> Div {
    div()
        .flex_col()
        .gap(px(PopupLayout::RESET_ROW_GAP))
        .child(labeled_line(
            &t!("codex.reset.type"),
            &row.reset_type,
            theme,
        ))
        .child(labeled_line(
            &t!("codex.reset.granted"),
            &row.granted_at,
            theme,
        ))
        .child(labeled_line(
            &t!("codex.reset.expires"),
            &row.expires_at,
            theme,
        ))
}

fn labeled_line(label: &str, value: &str, theme: &Theme) -> Div {
    div()
        .h(px(PopupLayout::RESET_LINE_HEIGHT))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(8.0))
        .child(
            div()
                .flex_shrink_0()
                .text_size(px(11.0))
                .line_height(relative(1.0))
                .text_color(theme.text.muted)
                .child(label.to_string()),
        )
        .child(
            div()
                .overflow_hidden()
                .text_size(px(11.0))
                .line_height(relative(1.0))
                .text_color(theme.text.primary)
                .whitespace_nowrap()
                .child(value.to_string()),
        )
}

fn reset_line(text: &str, color: gpui::Hsla, weight: FontWeight) -> Div {
    div()
        .h(px(PopupLayout::RESET_LINE_HEIGHT))
        .overflow_hidden()
        .text_size(px(11.0))
        .line_height(relative(1.0))
        .font_weight(weight)
        .text_color(color)
        .whitespace_nowrap()
        .child(text.to_string())
}
