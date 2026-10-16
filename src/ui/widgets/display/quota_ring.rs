//! 圆圈样式的配额卡：顶栏沿用圆点、标题和状态，圆环里只放数字，右边是用量历史折线。
//!
//! 顶栏占掉一截高度后，圆环仍尽量大，让数字成为环里唯一的内容。折线不再把窗口加高。
//! 弧从 12 点顺时针画，长度跟当前「剩余 / 已用」数字一致。纯余额没有总量，画满环。

use crate::application::{
    format_quota_card_detail_text, format_quota_card_display_text, format_quota_card_has_unit,
    format_quota_status_label, quota_ring_fill_ratio, HistoryChartView, QuotaDisplayViewState,
};
use crate::models::{PopupLayout, QuotaDisplayMode};
use crate::theme::Theme;
use gpui::{
    canvas, div, point, px, relative, Bounds, Div, FontWeight, Hsla, IntoElement, ParentElement,
    PathBuilder, Pixels, Styled, Window,
};

/// 圆环卡片把上下内边距收到 10px，给顶栏留位后圆环仍是 80px，整张卡落在 `CARD_HEIGHT` 里。
const RING_PAD_Y: f32 = 10.0;
const RING_SIZE: f32 = 80.0;
const RING_STROKE: f32 = 7.0;

pub(in crate::ui::widgets::display) fn render_quota_ring(
    quota_view: &QuotaDisplayViewState,
    theme: &Theme,
    display_mode: QuotaDisplayMode,
    history: Option<&HistoryChartView>,
) -> impl IntoElement {
    let quota = &quota_view.quota;
    let fill = theme.status_bar_color(quota_view.status_level);
    let badge = theme.status_badge_color(quota_view.status_level);
    let track = theme.status.progress_track;
    let ratio = quota_ring_fill_ratio(quota, display_mode);
    let display_text = format_quota_card_display_text(quota, display_mode);
    let has_unit = format_quota_card_has_unit(quota);
    let detail_text = format_quota_card_detail_text(quota_view);
    let status_label = format_quota_status_label(quota_view.status_level);
    let hover_bg = theme.bg.card_inner_hovered;

    super::quota_bar::render_quota_card_frame(quota, theme, hover_bg)
        .py(px(RING_PAD_Y))
        .child(super::quota_bar::render_quota_header_row(
            quota_view,
            badge,
            status_label,
            theme,
        ))
        .child(
            div()
                .w_full()
                .flex()
                .items_start()
                .gap(px(12.0))
                .child(render_ring(
                    ratio,
                    &display_text,
                    has_unit,
                    track,
                    fill,
                    theme,
                ))
                .child(render_ring_side(history, &detail_text, theme)),
        )
}

fn render_ring(
    ratio: Option<f32>,
    display_text: &str,
    has_unit: bool,
    track: Hsla,
    fill: Hsla,
    theme: &Theme,
) -> Div {
    let value_size = value_text_size(display_text);
    div()
        .relative()
        .flex_shrink_0()
        .w(px(RING_SIZE))
        .h(px(RING_SIZE))
        .child(
            div().w(px(RING_SIZE)).h(px(RING_SIZE)).child(
                canvas(
                    |_, _, _| {},
                    move |bounds, _, window, _| {
                        paint_ring(bounds, ratio, track, fill, window);
                    },
                )
                .size_full(),
            ),
        )
        .child(
            div()
                .absolute()
                .top(px(RING_STROKE))
                .left(px(RING_STROKE))
                .w(px(RING_SIZE - RING_STROKE * 2.0))
                .h(px(RING_SIZE - RING_STROKE * 2.0))
                .flex()
                .items_center()
                .justify_center()
                .child(render_ring_value(display_text, has_unit, value_size, theme)),
        )
}

fn render_ring_value(display_text: &str, has_unit: bool, value_size: f32, theme: &Theme) -> Div {
    let mut value = div().flex().items_end().justify_center().child(
        div()
            .text_size(px(value_size))
            .font_weight(FontWeight::BOLD)
            .text_color(theme.text.primary)
            .line_height(relative(1.0))
            .whitespace_nowrap()
            .child(display_text.to_string()),
    );
    if has_unit {
        value = value.child(
            div()
                .text_size(px((value_size * 0.5).max(11.0)))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text.secondary)
                .line_height(relative(1.0))
                .ml(px(1.0))
                .mb(px(2.0))
                .child("%"),
        );
    }
    value
}

fn render_ring_side(history: Option<&HistoryChartView>, detail_text: &str, theme: &Theme) -> Div {
    let mut side = div().flex_1().min_w_0().flex().flex_col();
    if let Some(history) = history {
        let plot = (RING_SIZE - PopupLayout::CARD_HISTORY_AXIS_CHROME).max(28.0);
        side = side.gap(px(PopupLayout::CARD_RING_DETAIL_GAP)).child(
            div()
                .h(px(RING_SIZE))
                .overflow_hidden()
                .child(super::render_history_line_chart(
                    &history.quota_key,
                    history.lines.clone(),
                    history.axis.clone(),
                    plot,
                    theme,
                )),
        );
        side = side.child(render_ring_detail(detail_text, theme));
    } else if !detail_text.is_empty() {
        side = side.h(px(RING_SIZE)).justify_center().child(
            div()
                .text_size(px(12.0))
                .line_height(relative(1.35))
                .text_color(theme.text.muted)
                .child(detail_text.to_string()),
        );
    }
    side
}

fn render_ring_detail(detail_text: &str, theme: &Theme) -> Div {
    div()
        .h(px(PopupLayout::CARD_RING_DETAIL_LINE))
        .overflow_hidden()
        .whitespace_nowrap()
        .text_size(px(11.0))
        .line_height(relative(1.0))
        .text_color(theme.text.muted)
        .child(detail_text.to_string())
}

fn value_text_size(text: &str) -> f32 {
    match text.chars().count() {
        0..=2 => 28.0,
        3 => 24.0,
        4..=5 => 16.0,
        _ => 13.0,
    }
}

/// 12 点为 0，顺时针。返回相对圆心的单位向量，y 向下为正。
fn ring_endpoint(turns: f32) -> (f32, f32) {
    let theta = turns * std::f32::consts::TAU;
    (theta.sin(), -theta.cos())
}

fn ring_point(cx: f32, cy: f32, radius: f32, turns: f32) -> (f32, f32) {
    let (x, y) = ring_endpoint(turns);
    (cx + radius * x, cy + radius * y)
}

fn paint_ring(
    bounds: Bounds<Pixels>,
    ratio: Option<f32>,
    track: Hsla,
    fill: Hsla,
    window: &mut Window,
) {
    let diameter = f32::from(bounds.size.width);
    let radius = (diameter - RING_STROKE) / 2.0 - 0.5;
    let cx = f32::from(bounds.origin.x) + diameter / 2.0;
    let cy = f32::from(bounds.origin.y) + diameter / 2.0;

    match ratio {
        None => paint_circle(cx, cy, radius, RING_STROKE, fill, window),
        Some(ratio) if ratio >= 0.995 => {
            paint_circle(cx, cy, radius, RING_STROKE, track, window);
            paint_circle(cx, cy, radius, RING_STROKE, fill, window);
        }
        Some(ratio) if ratio > 0.0 => {
            paint_circle(cx, cy, radius, RING_STROKE, track, window);
            paint_arc(cx, cy, radius, RING_STROKE, ratio, fill, window);
            let cap = RING_STROKE / 2.0;
            let (sx, sy) = ring_point(cx, cy, radius, 0.0);
            let (ex, ey) = ring_point(cx, cy, radius, ratio);
            paint_disk(sx, sy, cap, fill, window);
            paint_disk(ex, ey, cap, fill, window);
        }
        Some(_) => paint_circle(cx, cy, radius, RING_STROKE, track, window),
    }
}

fn paint_circle(cx: f32, cy: f32, radius: f32, stroke: f32, color: Hsla, window: &mut Window) {
    let mut builder = PathBuilder::stroke(px(stroke));
    let right = point(px(cx + radius), px(cy));
    let left = point(px(cx - radius), px(cy));
    builder.move_to(right);
    // 两个半圆拼满环。起点和终点重合时，单段 arc 会被丢掉。
    builder.arc_to(point(px(radius), px(radius)), px(0.0), false, true, left);
    builder.arc_to(point(px(radius), px(radius)), px(0.0), false, true, right);
    paint_built(builder, color, window);
}

fn paint_arc(
    cx: f32,
    cy: f32,
    radius: f32,
    stroke: f32,
    turns: f32,
    color: Hsla,
    window: &mut Window,
) {
    let (sx, sy) = ring_point(cx, cy, radius, 0.0);
    let (ex, ey) = ring_point(cx, cy, radius, turns);
    let mut builder = PathBuilder::stroke(px(stroke));
    builder.move_to(point(px(sx), px(sy)));
    // sweep=true：y 向下时为顺时针，从 12 点走向 3 点。
    builder.arc_to(
        point(px(radius), px(radius)),
        px(0.0),
        turns > 0.5,
        true,
        point(px(ex), px(ey)),
    );
    paint_built(builder, color, window);
}

fn paint_disk(cx: f32, cy: f32, radius: f32, color: Hsla, window: &mut Window) {
    let mut builder = PathBuilder::fill();
    let right = point(px(cx + radius), px(cy));
    let left = point(px(cx - radius), px(cy));
    builder.move_to(right);
    builder.arc_to(point(px(radius), px(radius)), px(0.0), false, true, left);
    builder.arc_to(point(px(radius), px(radius)), px(0.0), false, true, right);
    builder.close();
    paint_built(builder, color, window);
}

fn paint_built(builder: PathBuilder, color: Hsla, window: &mut Window) {
    if let Ok(path) = builder.build() {
        window.paint_path(path, color);
    }
}

#[cfg(test)]
mod tests {
    use super::ring_endpoint;

    #[test]
    fn ring_starts_at_top_and_turns_clockwise() {
        let (x, y) = ring_endpoint(0.0);
        assert!(x.abs() < 1e-4);
        assert!((y + 1.0).abs() < 1e-4);

        let (x, y) = ring_endpoint(0.25);
        assert!((x - 1.0).abs() < 1e-4);
        assert!(y.abs() < 1e-4);

        let (x, y) = ring_endpoint(0.5);
        assert!(x.abs() < 1e-4);
        assert!((y - 1.0).abs() < 1e-4);
    }
}
