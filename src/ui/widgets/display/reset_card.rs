//! 限流重置卡。
//!
//! 不复用额度卡：没有大数字、百分号、进度条和 OK/WARN/LOW 徽章。
//! 每一条只放重置类型和到期时间；完整时间信息通过悬浮提示查看。

use crate::application::{RateLimitResetSectionView, ResetExpiryUrgency};
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
        .child(reset_heading(section, theme));

    for (index, row) in section.rows.iter().enumerate() {
        card = card.child(render_reset_row(row, index, theme));
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

fn reset_heading(section: &RateLimitResetSectionView, theme: &Theme) -> Div {
    let color = section
        .rows
        .iter()
        .map(|row| row.expiry_urgency)
        .min_by_key(reset_urgency_rank)
        .map(|urgency| reset_urgency_color(urgency, theme))
        .unwrap_or(theme.text.muted);

    div()
        .w_full()
        .h(px(PopupLayout::RESET_LINE_HEIGHT))
        .flex()
        .items_center()
        .gap(px(6.0))
        .child(div().w(px(7.0)).h(px(7.0)).rounded_full().bg(color))
        .child(
            div()
                .overflow_hidden()
                .text_size(px(11.0))
                .line_height(relative(1.3))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.text.secondary)
                .whitespace_nowrap()
                .child(t!("codex.reset.heading").to_uppercase()),
        )
}

fn reset_urgency_rank(urgency: &ResetExpiryUrgency) -> u8 {
    match urgency {
        ResetExpiryUrgency::Critical => 0,
        ResetExpiryUrgency::Warning => 1,
        ResetExpiryUrgency::Safe => 2,
        ResetExpiryUrgency::Unknown => 3,
    }
}

fn reset_urgency_color(urgency: ResetExpiryUrgency, theme: &Theme) -> gpui::Hsla {
    match urgency {
        ResetExpiryUrgency::Critical => theme.status.error,
        ResetExpiryUrgency::Warning => theme.status.warning,
        ResetExpiryUrgency::Safe => theme.status.success,
        ResetExpiryUrgency::Unknown => theme.text.muted,
    }
}

fn render_reset_row(
    row: &crate::application::RateLimitResetRowView,
    index: usize,
    theme: &Theme,
) -> impl gpui::IntoElement {
    let expiry_color = reset_urgency_color(row.expiry_urgency, theme);
    let content = div()
        .h(px(PopupLayout::RESET_LINE_HEIGHT))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(8.0))
        .child(
            div()
                .flex_shrink_0()
                .text_size(px(13.0))
                .line_height(relative(1.0))
                .text_color(theme.text.primary)
                .child(row.reset_type.clone()),
        )
        .child(
            div()
                .overflow_hidden()
                .text_size(px(13.0))
                .line_height(relative(1.0))
                .text_color(expiry_color)
                .whitespace_nowrap()
                .child(row.expires_at.clone()),
        );

    crate::ui::widgets::with_multiline_tooltip(
        format!("rate-limit-reset-{index}"),
        &format!(
            "{}\n{}: {}\n{}: {}",
            row.reset_type,
            t!("codex.reset.granted"),
            row.granted_at,
            t!("codex.reset.expires"),
            row.expires_at
        ),
        theme,
        content,
    )
}

fn reset_line(text: &str, color: gpui::Hsla, weight: FontWeight) -> Div {
    div()
        .h(px(PopupLayout::RESET_LINE_HEIGHT))
        .overflow_hidden()
        .text_size(px(12.0))
        .line_height(relative(1.0))
        .font_weight(weight)
        .text_color(color)
        .whitespace_nowrap()
        .child(text.to_string())
}
