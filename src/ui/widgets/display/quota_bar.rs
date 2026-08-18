use std::time::Duration;

use crate::application::{
    format_quota_card_detail_text, format_quota_card_display_text, format_quota_card_has_unit,
    format_quota_card_mode_label, format_quota_status_label, QuotaDisplayViewState,
};
use crate::models::{QuotaDisplayMode, QuotaInfo, StatusLevel};
use crate::theme::Theme;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, ease_out_quint, linear_color_stop, multi_stop_linear_gradient, px, relative, Animation,
    AnimationExt, Div, ElementId, FontWeight, Hsla, InteractiveElement, IntoElement, ParentElement,
    Stateful, StyleRefinement, Styled,
};

/// 状态徽章颜色
fn status_badge_color(level: StatusLevel, theme: &Theme) -> Hsla {
    match level {
        StatusLevel::Green => theme.badge.healthy,
        StatusLevel::Yellow => theme.badge.degraded,
        StatusLevel::Red => theme.badge.offline,
    }
}

/// 进度条颜色（与状态对应）
fn bar_color(level: StatusLevel, theme: &Theme) -> Hsla {
    match level {
        StatusLevel::Green => theme.status.success,
        StatusLevel::Yellow => theme.status.warning,
        StatusLevel::Red => theme.status.error,
    }
}

fn render_quota_card_frame(
    quota: &QuotaInfo,
    theme: &Theme,
    hover_bg: Hsla,
) -> gpui::Stateful<Div> {
    div()
        .id(ElementId::Name(
            format!("quota-card-{}", quota.stable_key).into(),
        ))
        .w_full()
        .flex_col()
        .gap(px(6.0))
        .px(px(16.0))
        .py(px(14.0))
        .rounded(px(12.0))
        .bg(theme.bg.card_inner)
        .border_1()
        .border_color(theme.border.strong)
        .hover(move |style: StyleRefinement| style.bg(hover_bg))
}

fn render_quota_header_row(
    quota_view: &QuotaDisplayViewState,
    badge_color: Hsla,
    badge_label: String,
    theme: &Theme,
) -> Div {
    div()
        .w_full()
        .flex()
        .justify_between()
        .items_center()
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .flex_shrink_0()
                .child(div().w(px(7.0)).h(px(7.0)).rounded_full().bg(badge_color))
                .child(
                    div()
                        .overflow_hidden()
                        .text_size(px(11.0))
                        .line_height(relative(1.3))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.text.secondary)
                        .whitespace_nowrap()
                        .child(quota_view.label.to_uppercase()),
                ),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_size(px(10.0))
                .font_weight(FontWeight::BOLD)
                .text_color(badge_color)
                .px(px(8.0))
                .py(px(2.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(badge_color)
                .child(badge_label),
        )
}

fn render_quota_value_row(
    display_text: &str,
    mode_label: &str,
    has_unit: bool,
    theme: &Theme,
) -> Div {
    div()
        .w_full()
        .flex()
        .justify_between()
        .items_end()
        .child(
            div()
                .flex()
                .items_end()
                .flex_shrink_0()
                .child(
                    div()
                        .text_size(px(36.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(theme.text.primary)
                        .line_height(relative(1.0))
                        .whitespace_nowrap()
                        .child(display_text.to_string()),
                )
                .children(if has_unit {
                    Some(
                        div()
                            .text_size(px(18.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text.secondary)
                            .line_height(relative(1.0))
                            .ml(px(2.0))
                            .mb(px(6.0))
                            .child("%"),
                    )
                } else {
                    None
                }),
        )
        .child(
            div()
                .text_size(px(12.0))
                .text_color(theme.text.secondary)
                .line_height(relative(1.0))
                .mb(px(1.0))
                .child(mode_label.to_string()),
        )
}

fn render_quota_progress_row(
    quota: &QuotaInfo,
    fill_color: Hsla,
    generation: u64,
    theme: &Theme,
) -> Div {
    let remaining_pct = quota.percent_remaining();
    let target_ratio = if remaining_pct < 0.0 {
        0.0_f32
    } else {
        remaining_pct as f32 / 100.0
    };
    let anim_id = ElementId::Name(format!("quota-bar-{}-{}", quota.stable_key, generation).into());
    let gradient_start = theme.status.bar_gradient_start;
    let gradient_mid = theme.status.bar_gradient_mid;

    div()
        .w_full()
        .h(px(5.0))
        .mt(px(6.0))
        .mb(px(6.0))
        .bg(theme.status.progress_track)
        .rounded_full()
        .overflow_hidden()
        .child(
            div()
                .id("quota-bar-fill")
                .h_full()
                .rounded_full()
                .bg(multi_stop_linear_gradient(
                    90.,
                    &[
                        linear_color_stop(gradient_start, 0.),
                        linear_color_stop(gradient_mid, 0.5),
                        linear_color_stop(fill_color, 1.),
                    ],
                ))
                .with_animation(
                    anim_id,
                    Animation::new(Duration::from_millis(1000)).with_easing(ease_out_quint()),
                    move |el: Stateful<Div>, delta| el.w(relative(delta * target_ratio)),
                ),
        )
}

fn render_quota_detail_row(detail_text: &str, theme: &Theme) -> Div {
    div().flex().items_center().gap(px(4.0)).mt(px(12.0)).child(
        div()
            .text_size(px(11.0))
            .text_color(theme.text.muted)
            .child(detail_text.to_string()),
    )
}

/// Lumina Bar 风格的 quota 卡片
pub(crate) fn render_quota_bar(
    quota_view: &QuotaDisplayViewState,
    theme: &Theme,
    generation: u64,
    display_mode: QuotaDisplayMode,
) -> impl IntoElement {
    let q = &quota_view.quota;
    let status = q.status_level();
    let badge_color = status_badge_color(status, theme);
    let badge_label = format_quota_status_label(status);
    let fill_color = bar_color(status, theme);
    let is_balance = q.is_balance_only();
    let display_text = format_quota_card_display_text(q, display_mode);
    let mode_label = format_quota_card_mode_label(is_balance, display_mode);
    let has_unit = format_quota_card_has_unit(q);
    let detail_text = format_quota_card_detail_text(quota_view);
    let hover_bg = theme.bg.card_inner_hovered;
    let card = render_quota_card_frame(q, theme, hover_bg);

    card.child(render_quota_header_row(
        quota_view,
        badge_color,
        badge_label,
        theme,
    ))
    .child(render_quota_value_row(
        &display_text,
        &mode_label,
        has_unit,
        theme,
    ))
    .when(!is_balance, |card: Stateful<Div>| {
        card.child(render_quota_progress_row(q, fill_color, generation, theme))
    })
    .child(render_quota_detail_row(&detail_text, theme))
}
