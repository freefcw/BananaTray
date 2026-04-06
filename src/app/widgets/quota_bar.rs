use std::time::Duration;

use crate::models::{QuotaDisplayMode, QuotaInfo, QuotaType, StatusLevel};
use crate::theme::Theme;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use rust_i18n::t;

/// 状态徽章文本
fn status_badge_label(level: StatusLevel) -> &'static str {
    match level {
        StatusLevel::Green => "HEALTHY",
        StatusLevel::Yellow => "DEGRADED",
        StatusLevel::Red => "OFFLINE",
    }
}

/// 状态徽章颜色
fn status_badge_color(level: StatusLevel, theme: &Theme) -> Hsla {
    match level {
        StatusLevel::Green => theme.badge_healthy,
        StatusLevel::Yellow => theme.badge_degraded,
        StatusLevel::Red => theme.badge_offline,
    }
}

/// 进度条颜色（与状态对应）
fn bar_color(level: StatusLevel, theme: &Theme) -> Hsla {
    match level {
        StatusLevel::Green => theme.status_success,
        StatusLevel::Yellow => theme.status_warning,
        StatusLevel::Red => theme.status_error,
    }
}

/// Lumina Bar 风格的 quota 卡片
pub(crate) fn render_quota_bar(
    q: &QuotaInfo,
    theme: &Theme,
    generation: u64,
    display_mode: QuotaDisplayMode,
) -> impl IntoElement {
    let status = q.status_level();
    let badge_color = status_badge_color(status, theme);
    let badge_label = status_badge_label(status);
    let fill_color = bar_color(status, theme);
    let is_balance = q.is_balance_only();

    // 主显示文本
    let display_text = if is_balance {
        // 余额模式：直接显示余额（保留2位小数）
        let balance = q.remaining_balance.unwrap_or(0.0);
        if matches!(q.quota_type, QuotaType::Credit) {
            format!("${:.2}", balance)
        } else {
            format!("{:.2}", balance)
        }
    } else {
        // 传统模式
        let remaining_pct = q.percent_remaining();
        match (&q.quota_type, display_mode) {
            (QuotaType::Credit, QuotaDisplayMode::Remaining) => {
                let remaining = q.limit - q.used;
                if remaining >= 0.0 {
                    format!("${:.0}", remaining)
                } else {
                    format!("-${:.0}", -remaining)
                }
            }
            (QuotaType::Credit, QuotaDisplayMode::Used) => {
                format!("${:.0}", q.used)
            }
            (_, QuotaDisplayMode::Remaining) => {
                let pct = remaining_pct.max(0.0);
                format!("{:.0}", pct)
            }
            (_, QuotaDisplayMode::Used) => {
                let pct = q.percentage().clamp(0.0, 100.0);
                format!("{:.0}", pct)
            }
        }
    };

    // 模式标签
    let mode_label = if is_balance {
        t!("quota.mode.balance").to_string()
    } else {
        match display_mode {
            QuotaDisplayMode::Remaining => t!("quota.mode.remaining").to_string(),
            QuotaDisplayMode::Used => t!("quota.mode.used").to_string(),
        }
    };

    // 余额模式无 % 单位；传统模式 Credit 也无 % 单位
    let has_unit = !is_balance && !matches!(q.quota_type, QuotaType::Credit);

    let hover_bg = theme.bg_card_inner_hovered;

    let card = div()
        .id(ElementId::Name(format!("quota-card-{}", q.label).into()))
        .w_full()
        .flex_col()
        .gap(px(6.0))
        .px(px(16.0))
        .py(px(14.0))
        .rounded(px(12.0))
        .bg(theme.bg_card_inner)
        .border_1()
        .border_color(theme.border_strong)
        .hover(move |style| style.bg(hover_bg));

    card
        // ── 第一行：● MODEL-NAME + [HEALTHY] ──
        .child(
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
                        // 状态指示点
                        .child(div().w(px(7.0)).h(px(7.0)).rounded_full().bg(badge_color))
                        .child(
                            div()
                                .overflow_hidden()
                                .text_size(px(11.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme.text_secondary)
                                .whitespace_nowrap()
                                .child(q.label.to_uppercase()),
                        ),
                )
                // 状态徽章
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
                ),
        )
        // ── 第二行：大号数字 + "Remaining" / "Balance" ──
        .child(
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
                                .text_color(theme.text_primary)
                                .line_height(relative(1.0))
                                .whitespace_nowrap()
                                .child(display_text),
                        )
                        .children(if has_unit {
                            Some(
                                div()
                                    .text_size(px(18.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.text_secondary)
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
                        .text_color(theme.text_secondary)
                        .line_height(relative(1.0))
                        .mb(px(1.0))
                        .child(mode_label),
                ),
        )
        // ── 第三行：进度条（余额模式隐藏） ──
        .when(!is_balance, |card: Stateful<Div>| {
            let remaining_pct = q.percent_remaining();
            let is_over_limit = remaining_pct < 0.0;
            let target_ratio = if is_over_limit {
                0.0_f32
            } else {
                remaining_pct as f32 / 100.0
            };
            let anim_id = ElementId::Name(format!("quota-bar-{}-{}", q.label, generation).into());
            let gradient_start: Hsla = rgb(0x6366f1).into();
            let gradient_mid: Hsla = rgb(0x06b6d4).into();
            let gradient_end = fill_color;

            card.child(
                div()
                    .w_full()
                    .h(px(5.0))
                    .mt(px(6.0))
                    .mb(px(6.0))
                    .bg(theme.progress_track)
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
                                    linear_color_stop(gradient_end, 1.),
                                ],
                            ))
                            .with_animation(
                                anim_id,
                                Animation::new(Duration::from_millis(1000))
                                    .with_easing(ease_out_quint()),
                                move |el, delta| el.w(relative(delta * target_ratio)),
                            ),
                    ),
            )
        })
        // ── 第四行：详情文本 ──
        .child({
            // 余额模式显示已用额度，传统模式显示 detail_text
            let detail_str = if is_balance {
                let used_text = q.usage_detail_text();
                if used_text.is_empty() {
                    q.detail_text.clone().unwrap_or_default()
                } else if let Some(ref detail) = q.detail_text {
                    format!("{} · {}", used_text, detail)
                } else {
                    used_text
                }
            } else {
                q.detail_text.clone().unwrap_or_default()
            };
            div().flex().items_center().gap(px(4.0)).mt(px(12.0)).child(
                div()
                    .text_size(px(11.0))
                    .text_color(theme.text_muted)
                    .child(detail_str),
            )
        })
}
