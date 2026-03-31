use crate::models::{QuotaInfo, StatusLevel};
use crate::theme::Theme;
use gpui::*;

pub(crate) fn render_quota_bar(q: &QuotaInfo, show_divider: bool, theme: &Theme) -> Div {
    let remaining_pct = q.percent_remaining();

    // 判断是否超出配额（负数剩余）
    let is_over_limit = remaining_pct < 0.0;

    let bar_fill = match q.status_level() {
        StatusLevel::Green => theme.status_success,
        StatusLevel::Yellow => theme.status_warning,
        StatusLevel::Red => theme.status_error,
    };

    let title_color = theme.text_primary;
    let secondary_color = theme.text_secondary;

    // 剩余文本颜色：负数时用红色
    let remaining_color = if is_over_limit {
        theme.status_error
    } else {
        secondary_color
    };

    // 进度条显示：如果超出配额，显示空条（或显示超出部分为红色）
    let bar_width = if is_over_limit {
        relative(0.0) // 完全耗尽
    } else {
        relative(remaining_pct as f32 / 100.0)
    };

    let row = div()
        .flex_col()
        .gap(px(2.0))
        .child(
            div()
                .flex()
                .justify_between()
                .items_center()
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(title_color)
                        .child(q.label.clone()),
                )
                .child(div().text_size(px(11.0)).text_color(secondary_color).child(
                    if let Some(ref reset) = q.reset_at {
                        reset.clone()
                    } else {
                        String::new()
                    },
                )),
        )
        .child(
            div()
                .w_full()
                .h(px(6.0))
                .bg(theme.progress_track)
                .rounded_full()
                .border_1()
                .border_color(theme.border_subtle)
                .overflow_hidden()
                .child(div().w(bar_width).h_full().bg(bar_fill).rounded_full()),
        )
        .child(
            div()
                .text_size(px(11.0))
                .text_color(remaining_color)
                .child(q.remaining_text()),
        );

    if show_divider {
        row.mt(px(4.0))
    } else {
        row
    }
}
