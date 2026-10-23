//! 限流重置卡。
//!
//! 不复用额度卡：没有大数字、百分号、进度条和 OK/WARN/LOW 徽章。
//! 每一条只放重置类型和到期时间；完整时间信息通过悬浮提示查看。

use crate::application::{RateLimitResetSectionView, ResetExpiryUrgency};
use crate::models::PopupLayout;
use crate::theme::Theme;
use gpui::{
    div, hsla, px, relative, Div, ElementId, FontWeight, Hsla, InteractiveElement, ParentElement,
    Stateful, Styled, TextAlign,
};
use rust_i18n::t;

pub(crate) fn render_rate_limit_resets(
    section: &RateLimitResetSectionView,
    theme: &Theme,
) -> Stateful<Div> {
    let mut card = div()
        .id(ElementId::Name("rate-limit-resets".into()))
        .w_full()
        .flex()
        .flex_col()
        .gap(px(PopupLayout::RESET_CARD_GAP))
        .px(px(16.0))
        .pt(px(PopupLayout::RESET_CARD_PAD_TOP))
        .pb(px(PopupLayout::RESET_CARD_PAD_BOTTOM))
        .rounded(px(12.0))
        .bg(theme.bg.card_inner)
        .border_1()
        .border_color(theme.border.subtle)
        .child(reset_heading(section, theme));

    let remaining_width = remaining_column_width(&section.rows);
    for (index, row) in section.rows.iter().enumerate() {
        card = card.child(render_reset_row(row, index, remaining_width, theme));
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
        .map(|urgency| reset_time_color(urgency, theme))
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

fn reset_urgency_color(urgency: ResetExpiryUrgency, theme: &Theme) -> Hsla {
    match urgency {
        ResetExpiryUrgency::Critical => theme.status.error,
        ResetExpiryUrgency::Warning => theme.status.warning,
        ResetExpiryUrgency::Safe => theme.status.success,
        ResetExpiryUrgency::Unknown => theme.text.muted,
    }
}

/// 重置时间沿用状态色相，但把饱和度压低。额度用的 `#22c55e` 铺在一列时间上会发荧光。
fn reset_time_color(urgency: ResetExpiryUrgency, theme: &Theme) -> Hsla {
    let color = reset_urgency_color(urgency, theme);
    if matches!(urgency, ResetExpiryUrgency::Unknown) {
        return color;
    }
    hsla(color.h, color.s * 0.55, color.l * 0.72 + 0.16, color.a)
}

/// 天数列只留这张卡里最长一条的宽度。「7 天」和「22 天」右对齐，日期不会被固定 72px 推开。
fn remaining_column_width(rows: &[crate::application::RateLimitResetRowView]) -> f32 {
    const FONT: f32 = 13.0;
    rows.iter()
        .filter_map(|row| row.expires_in.as_deref())
        .map(|text| {
            text.chars()
                .map(|ch| if ch.is_ascii() { FONT * 0.62 } else { FONT })
                .sum::<f32>()
        })
        .fold(0.0, f32::max)
        .ceil()
        + 2.0
}

fn render_reset_row(
    row: &crate::application::RateLimitResetRowView,
    index: usize,
    remaining_width: f32,
    theme: &Theme,
) -> impl gpui::IntoElement {
    let expiry_color = reset_time_color(row.expiry_urgency, theme);
    let mut expiry = div()
        .flex()
        .items_center()
        .flex_shrink_0()
        .gap(px(8.0))
        .child(
            div()
                .text_size(px(13.0))
                .line_height(relative(1.0))
                .text_color(expiry_color)
                .whitespace_nowrap()
                .child(row.expires_on.clone()),
        );
    if let Some(remaining) = &row.expires_in {
        expiry = expiry.child(
            div()
                .w(px(remaining_width))
                .flex_shrink_0()
                .text_size(px(13.0))
                .line_height(relative(1.0))
                .text_align(TextAlign::Right)
                .text_color(expiry_color)
                .whitespace_nowrap()
                .child(remaining.clone()),
        );
    }
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
        .child(expiry);

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
        // 行高必须撑满整行：1.0 倍行高时字形按字体 ascent+descent 居中会伸出行框顶边，
        // 中文字形更高，顶部被 overflow_hidden 裁掉。
        .line_height(px(PopupLayout::RESET_LINE_HEIGHT))
        .font_weight(weight)
        .text_color(color)
        .whitespace_nowrap()
        .child(text.to_string())
}
