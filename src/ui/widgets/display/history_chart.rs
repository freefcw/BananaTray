//! 用量历史折线图：设置页与托盘弹窗共用，只负责把归一化点画出来。

use crate::application::HistorySegmentView;
use crate::theme::Theme;
use gpui::{
    canvas, div, fill, point, px, size, Bounds, Div, Hsla, ParentElement, PathBuilder, Pixels,
    Point, Styled, Window,
};

pub(crate) fn render_history_line_chart(
    segments: Vec<HistorySegmentView>,
    height: f32,
    theme: &Theme,
) -> Div {
    let color = theme.text.accent;
    div().h(px(height)).w_full().child(
        canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                paint_segments(bounds, &segments, color, window);
            },
        )
        .size_full(),
    )
}

fn paint_segments(
    bounds: Bounds<Pixels>,
    segments: &[HistorySegmentView],
    color: Hsla,
    window: &mut Window,
) {
    for segment in segments {
        paint_segment(bounds, &segment.points, color, window);
    }
}

fn paint_segment(
    bounds: Bounds<Pixels>,
    points: &[crate::application::HistoryPointView],
    color: Hsla,
    window: &mut Window,
) {
    let mut run = Vec::new();
    for point in points {
        if point.gap_before && !run.is_empty() {
            paint_run(&run, color, window);
            run.clear();
        }
        run.push(chart_point(bounds, point.x_ratio, point.y_ratio));
    }
    if !run.is_empty() {
        paint_run(&run, color, window);
    }
}

fn paint_run(points: &[Point<Pixels>], color: Hsla, window: &mut Window) {
    if let [only] = points {
        window.paint_quad(fill(
            Bounds {
                origin: point(only.x - px(1.5), only.y - px(1.5)),
                size: size(px(3.0), px(3.0)),
            },
            color,
        ));
        return;
    }
    let mut builder = PathBuilder::stroke(px(1.5));
    builder.move_to(points[0]);
    for point in &points[1..] {
        builder.line_to(*point);
    }
    if let Ok(path) = builder.build() {
        window.paint_path(path, color);
    }
}

fn chart_point(bounds: Bounds<Pixels>, x_ratio: f64, y_ratio: f64) -> Point<Pixels> {
    let x = bounds.origin.x + bounds.size.width * (x_ratio as f32);
    let y = bounds.origin.y + bounds.size.height * (1.0 - y_ratio as f32);
    point(x, y)
}
