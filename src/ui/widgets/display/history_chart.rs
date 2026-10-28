//! 用量历史折线图：设置页与托盘弹窗共用。
//! 绘图区只画归一化点；坐标文字来自 `HistoryChartAxisView`。
//! 指针停在折线区域里时，十字和读数按这一帧的鼠标位置现算，不进应用状态。

use crate::application::{
    history_hover_lines, hover_chip_origin, HistoryChartView, HistoryHoverHit, HistoryLineView,
    HistoryPeriodView, HistoryPointView,
};
use crate::models::PopupLayout;
use crate::theme::Theme;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    canvas, div, fill, point, px, quad, relative, size, App, BorderStyle, Bounds, Div, Hsla,
    InteractiveElement, ParentElement, PathBuilder, Pixels, Point, SharedString,
    StatefulInteractiveElement, Styled, Window,
};

/// 折线右侧缩进，避免贴着绘图区右缘被裁掉。
const PLOT_INSET_X: f32 = 2.0;
/// 绘图区上下内缩，为 100%/0% 处的 2px 粗折线与 5px 悬停圆点留出呼吸空间，并与 10px 纵轴文字中心对齐。
const PLOT_INSET_Y: f32 = 6.0;
/// 纵轴数字离绘图区左缘的距离。
const LABEL_PAD: f32 = 6.0;
/// 10px 纵轴文字（line-height 1.0）与 `PLOT_INSET_Y` 参考线中心对齐的顶/底偏移。
const Y_TICK_EDGE_OFFSET: f32 = PLOT_INSET_Y - 5.0;
/// 台阶笔画。比 1px 基线略粗，小图里仍能看清下降。
const STEP_STROKE: f32 = 2.0;
/// 台阶拐角的圆角半径。短台阶会把它收到线段的一半，避免两段圆角叠在一起。
const STEP_RADIUS: f32 = 4.0;
/// 台阶下方填充的不透明度。只铺在折线到基线之间。
const STEP_FILL: f32 = 0.2;
/// 持平无波动序列的填充衰减系数，避免水平直线下方形成笨重色块。
const FLAT_FILL_FACTOR: f32 = 0.45;
/// 断档桥接下方过渡填充相对主填充的衰减系数。
const GAP_FILL_FACTOR: f32 = 0.5;
/// 断档虚线比台阶细、更淡，读作"这段没有数据"。
const GAP_STROKE: f32 = 1.5;
const GAP_OPACITY: f32 = 0.5;
/// 本周期图理想配速参考线的线宽与不透明度。
const PACE_STROKE: f32 = 1.25;
const PACE_OPACITY: f32 = 0.45;
/// 圆点半径是 2.5px。再留一点，孤立采样也能被点中。
const HOVER_HIT_PX: f64 = 4.0;
const CHIP_FONT: f32 = 10.0;
const CHIP_LINE: f32 = 13.0;
const CHIP_PAD_X: f32 = 5.0;
const CHIP_PAD_Y: f32 = 2.0;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct HistoryChartOptions {
    /// 首条折线的颜色覆盖（如配额卡片处于注意/偏低状态时跟随状态色）
    pub primary_line_color: Option<Hsla>,
    /// 当序列完全持平（`y_min` 为空）时是否隐藏纵轴唯一刻度（避免与圆环内大数字重复）
    pub suppress_flat_y_tick: bool,
}

#[derive(Clone, Copy)]
struct LinePaint {
    color: Hsla,
    fill_opacity: f32,
}

struct HoverColors {
    guide: Hsla,
    chip_bg: Hsla,
    text: Hsla,
}

pub(crate) fn history_line_color(index: usize, theme: &Theme) -> Hsla {
    let palette = [
        theme.text.accent,
        theme.status.success,
        theme.status.warning,
        theme.status.bar_gradient_mid,
        theme.status.bar_gradient_start,
        theme.log.info,
    ];
    palette[index % palette.len()]
}

pub(crate) fn render_history_line_chart(
    chart: &HistoryChartView,
    plot_height: f32,
    theme: &Theme,
) -> Div {
    render_history_line_chart_with_options(
        chart,
        plot_height,
        HistoryChartOptions::default(),
        theme,
    )
}

pub(crate) fn render_history_line_chart_with_options(
    chart: &HistoryChartView,
    plot_height: f32,
    options: HistoryChartOptions,
    theme: &Theme,
) -> Div {
    let chart_id = &chart.quota_key;
    let lines = chart.lines.clone();
    let axis = chart.axis.clone();
    let period = chart.period.clone();
    let is_flat = axis.y_min.is_empty();
    let frame = theme.border.subtle;
    let axis_color = theme.border.strong;
    let label = theme.text.muted;
    let colors = HoverColors {
        guide: frame,
        chip_bg: theme.bg.card,
        text: theme.text.primary,
    };
    let paint_theme = theme.clone();
    let x_start_ms = axis.x_start_ms;
    let x_end_ms = axis.x_end_ms;
    let hover_lines = lines.clone();
    let y_max_text = if is_flat && options.suppress_flat_y_tick {
        ""
    } else {
        axis.y_max.as_str()
    };
    let fill_opacity = chart_fill_opacity(period.is_none(), lines.len(), is_flat);
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(PopupLayout::CARD_HISTORY_AXIS_GAP))
        .child(
            div()
                .id(format!("history-chart-{chart_id}"))
                .relative()
                .h(px(plot_height))
                .w_full()
                .on_mouse_move(|_, window, _| {
                    window.refresh();
                })
                .on_hover(|_, window, _| {
                    window.refresh();
                })
                .child(
                    div().h(px(plot_height)).w_full().child(
                        canvas(
                            move |bounds, window, _| {
                                hover_hit(bounds, window, &hover_lines, x_start_ms, x_end_ms)
                            },
                            move |bounds, hover, window, cx| {
                                if let Some(hit) = &hover {
                                    paint_hover_guides(bounds, hit, frame, window);
                                }
                                if let Some(period) = &period {
                                    paint_period_guides(bounds, period, &paint_theme, window);
                                }
                                paint_lines(
                                    bounds,
                                    &lines,
                                    fill_opacity,
                                    options.primary_line_color,
                                    &paint_theme,
                                    window,
                                );
                                paint_baseline(bounds, axis_color, window);
                                if let Some(hit) = hover {
                                    paint_hover_marker(
                                        bounds,
                                        &hit,
                                        options.primary_line_color,
                                        &paint_theme,
                                        colors,
                                        window,
                                        cx,
                                    );
                                }
                            },
                        )
                        .size_full(),
                    ),
                )
                .child(y_ticks(
                    y_max_text,
                    &axis.y_min,
                    chart.period.as_ref().map(|period| period.y_mid.as_str()),
                    plot_height,
                    label,
                )),
        )
        .child(x_ticks(&axis.x_start, &axis.x_end, label))
}

fn hover_hit(
    bounds: Bounds<Pixels>,
    window: &Window,
    lines: &[HistoryLineView],
    x_start_ms: i64,
    x_end_ms: i64,
) -> Option<HistoryHoverHit> {
    let plot = line_plot_bounds(bounds);
    let mouse = window.mouse_position();
    if !plot.contains(&mouse) {
        return None;
    }
    let ratio = (mouse.x - plot.origin.x) / plot.size.width;
    let width = f64::from(plot.size.width / px(1.0)).max(1.0);
    let span = (x_end_ms - x_start_ms).max(1) as f64;
    Some(history_hover_lines(
        lines,
        x_start_ms,
        x_end_ms,
        f64::from(ratio),
        span / width * HOVER_HIT_PX,
    ))
}

fn y_ticks(top: &str, bottom: &str, mid: Option<&str>, plot_height: f32, color: Hsla) -> Div {
    // 盖在绘图框里面，和框的左缘对齐。单独占一列会把整张图往右推。
    let column = div()
        .absolute()
        .top(px(0.0))
        .left(px(LABEL_PAD))
        .w(px(PopupLayout::CARD_HISTORY_Y_GUTTER))
        .h(px(plot_height))
        .text_size(px(10.0))
        .line_height(relative(1.0))
        .text_color(color);
    if bottom.is_empty() {
        column.flex().items_center().child(top.to_string())
    } else {
        column
            .child(edge_tick(top, true, color))
            .child(edge_tick(bottom, false, color))
            .when_some(mid, |column, mid| {
                column.child(mid_tick(mid, plot_height, color))
            })
    }
}

/// 本周期图的纵轴中线刻度，垂直居中在绘图区中线上。
fn mid_tick(text: &str, plot_height: f32, color: Hsla) -> Div {
    div()
        .absolute()
        .left(px(0.0))
        // 10px 文字（line-height 1.0）中心对到绘图区中线。
        .top(px(plot_height / 2.0 - 5.0))
        .flex()
        .text_size(px(10.0))
        .line_height(relative(1.0))
        .text_color(color)
        .child(text.to_string())
}

fn edge_tick(text: &str, top: bool, color: Hsla) -> Div {
    let tick = div()
        .absolute()
        .left(px(0.0))
        .flex()
        .text_size(px(10.0))
        .line_height(relative(1.0))
        .text_color(color)
        .child(text.to_string());
    if top {
        tick.top(px(Y_TICK_EDGE_OFFSET))
    } else {
        tick.bottom(px(Y_TICK_EDGE_OFFSET))
    }
}

fn x_ticks(start: &str, end: &str, color: Hsla) -> Div {
    let inset = LABEL_PAD + PopupLayout::CARD_HISTORY_Y_GUTTER;
    div()
        .h(px(PopupLayout::CARD_HISTORY_AXIS_TIME))
        .w_full()
        .pl(px(inset))
        .pr(px(PLOT_INSET_X))
        .flex()
        .justify_between()
        .items_center()
        .text_size(px(10.0))
        .line_height(relative(1.0))
        .text_color(color)
        .child(start.to_string())
        .child(end.to_string())
}

fn resolve_line_color(color_index: usize, primary_override: Option<Hsla>, theme: &Theme) -> Hsla {
    if color_index == 0 {
        if let Some(color) = primary_override {
            return color;
        }
    }
    history_line_color(color_index, theme)
}

fn chart_fill_opacity(allow_fill: bool, line_count: usize, is_flat: bool) -> f32 {
    // 多条折线同图时不叠加底部色块填充，避免蓝/绿/橙多重半透明色叠成浑浊暗色。
    // 本周期图有配速虚线，线下也不铺填充。持平无波动序列把填充压淡，避免抢夺视觉焦点。
    if !allow_fill || line_count != 1 {
        return 0.0;
    }
    if is_flat {
        STEP_FILL * FLAT_FILL_FACTOR
    } else {
        STEP_FILL
    }
}

fn paint_lines(
    bounds: Bounds<Pixels>,
    lines: &[HistoryLineView],
    fill_opacity: f32,
    primary_override: Option<Hsla>,
    theme: &Theme,
    window: &mut Window,
) {
    for line in lines {
        let style = LinePaint {
            color: resolve_line_color(line.color_index, primary_override, theme),
            fill_opacity,
        };
        for segment in &line.segments {
            paint_segment(bounds, &segment.points, style, window);
        }
    }
}

fn paint_segment(
    bounds: Bounds<Pixels>,
    points: &[HistoryPointView],
    style: LinePaint,
    window: &mut Window,
) {
    let mut run: Vec<&HistoryPointView> = Vec::new();
    for point in points {
        if point.gap_before && !run.is_empty() {
            paint_run(bounds, &run, style, window);
            paint_gap_bridge(bounds, run[run.len() - 1], point, style, window);
            run.clear();
        }
        run.push(point);
    }
    if !run.is_empty() {
        paint_run(bounds, &run, style, window);
    }
}

/// 断档不留白：用淡虚线按台阶形状接上，先保持断前的值，到恢复时刻再落到新值。
/// 当折线有底部填充时，断档下方铺一层更淡的过渡填充，避免平线在断档处被切成两块孤立色砖。
fn paint_gap_bridge(
    bounds: Bounds<Pixels>,
    before: &HistoryPointView,
    after: &HistoryPointView,
    style: LinePaint,
    window: &mut Window,
) {
    let from = chart_point(bounds, before.x_ratio, before.y_ratio);
    let to = chart_point(bounds, after.x_ratio, after.y_ratio);
    if style.fill_opacity > 0.0 && to.x > from.x {
        let plot = line_plot_bounds(bounds);
        let baseline = plot.origin.y + plot.size.height;
        let mut fill_path = PathBuilder::fill();
        fill_path.move_to(point(from.x, baseline));
        fill_path.line_to(from);
        fill_path.line_to(point(to.x, from.y));
        fill_path.line_to(point(to.x, baseline));
        fill_path.close();
        if let Ok(path) = fill_path.build() {
            window.paint_path(
                path,
                style.color.opacity(style.fill_opacity * GAP_FILL_FACTOR),
            );
        }
    }
    let mut builder = PathBuilder::stroke(px(GAP_STROKE)).dash_array(&[px(3.0), px(3.0)]);
    builder.move_to(from);
    builder.line_to(point(to.x, from.y));
    if !same_pixel(from.y / px(1.0), to.y / px(1.0)) {
        builder.line_to(to);
    }
    if let Ok(path) = builder.build() {
        window.paint_path(path, style.color.opacity(GAP_OPACITY));
    }
}

fn paint_run(
    bounds: Bounds<Pixels>,
    points: &[&HistoryPointView],
    style: LinePaint,
    window: &mut Window,
) {
    // 阶梯：保持上一个采样的值，到下一个不同的值再跳。相同用量并成一段平线。
    // 斜线会把两次采样之间画成渐变。采样点只在悬停时标出。
    let samples: Vec<SamplePoint> = points
        .iter()
        .map(|point| {
            let at = chart_point(bounds, point.x_ratio, point.y_ratio);
            SamplePoint {
                x: at.x / px(1.0),
                y: at.y / px(1.0),
                value: point.y,
            }
        })
        .collect();
    let vertices = plateau_vertices(&samples);
    if vertices.len() < 2 {
        return;
    }
    let commands = rounded_step(&vertices, STEP_RADIUS);
    let plot = line_plot_bounds(bounds);
    let baseline = (plot.origin.y + plot.size.height) / px(1.0);
    let first = vertices[0];
    let last = vertices[vertices.len() - 1];

    if style.fill_opacity > 0.0 {
        let mut fill_path = PathBuilder::fill();
        fill_path.move_to(point(px(first.0), px(baseline)));
        fill_path.line_to(point(px(first.0), px(first.1)));
        apply_step(&mut fill_path, &commands);
        fill_path.line_to(point(px(last.0), px(baseline)));
        fill_path.close();
        if let Ok(path) = fill_path.build() {
            window.paint_path(path, style.color.opacity(style.fill_opacity));
        }
    }

    let mut stroke = PathBuilder::stroke(px(STEP_STROKE));
    stroke.move_to(point(px(first.0), px(first.1)));
    apply_step(&mut stroke, &commands);
    if let Ok(path) = stroke.build() {
        window.paint_path(path, style.color);
    }
}

struct SamplePoint {
    x: f32,
    y: f32,
    value: f64,
}

/// 用量没变就延长当前平台，变了才在新采样的时刻拐下去。
fn plateau_vertices(samples: &[SamplePoint]) -> Vec<(f32, f32)> {
    let mut vertices = Vec::new();
    let Some(first) = samples.first() else {
        return vertices;
    };
    vertices.push((first.x, first.y));
    let mut level = first.value;
    for sample in samples.iter().skip(1) {
        if same_quota(sample.value, level) {
            extend_plateau(&mut vertices, sample.x);
            continue;
        }
        extend_plateau(&mut vertices, sample.x);
        push_vertex(&mut vertices, sample.x, sample.y);
        level = sample.value;
    }
    vertices
}

fn extend_plateau(vertices: &mut Vec<(f32, f32)>, x: f32) {
    let Some(last) = vertices.last().copied() else {
        return;
    };
    let continues = vertices
        .len()
        .checked_sub(2)
        .and_then(|index| vertices.get(index))
        .is_some_and(|previous| same_pixel(previous.1, last.1));
    if continues {
        vertices.last_mut().unwrap().0 = x;
    } else {
        push_vertex(vertices, x, last.1);
    }
}

fn push_vertex(vertices: &mut Vec<(f32, f32)>, x: f32, y: f32) {
    if vertices
        .last()
        .is_some_and(|last| same_pixel(last.0, x) && same_pixel(last.1, y))
    {
        return;
    }
    vertices.push((x, y));
}

fn same_quota(left: f64, right: f64) -> bool {
    let scale = left.abs().max(right.abs()).max(1.0);
    (left - right).abs() <= scale * 1e-6
}

fn same_pixel(left: f32, right: f32) -> bool {
    (left - right).abs() < 0.01
}

#[derive(Debug, PartialEq)]
enum StepCommand {
    Line(f32, f32),
    Curve(f32, f32, f32, f32),
}

/// 从第一个顶点之后开始。直线拐角收成二次曲线，控制点仍在原来的拐角上。
fn rounded_step(vertices: &[(f32, f32)], radius: f32) -> Vec<StepCommand> {
    let mut commands = Vec::new();
    if vertices.len() < 2 {
        return commands;
    }
    if vertices.len() == 2 {
        commands.push(StepCommand::Line(vertices[1].0, vertices[1].1));
        return commands;
    }
    for index in 1..vertices.len() - 1 {
        let prev = vertices[index - 1];
        let corner = vertices[index];
        let next = vertices[index + 1];
        let in_dx = corner.0 - prev.0;
        let in_dy = corner.1 - prev.1;
        let out_dx = next.0 - corner.0;
        let out_dy = next.1 - corner.1;
        let in_len = (in_dx * in_dx + in_dy * in_dy).sqrt();
        let out_len = (out_dx * out_dx + out_dy * out_dy).sqrt();
        let cross = in_dx * out_dy - in_dy * out_dx;
        if in_len < 0.5 || out_len < 0.5 || cross.abs() < 0.5 {
            commands.push(StepCommand::Line(corner.0, corner.1));
            continue;
        }
        let bend = radius.min(in_len / 2.0).min(out_len / 2.0);
        commands.push(StepCommand::Line(
            corner.0 - in_dx / in_len * bend,
            corner.1 - in_dy / in_len * bend,
        ));
        commands.push(StepCommand::Curve(
            corner.0 + out_dx / out_len * bend,
            corner.1 + out_dy / out_len * bend,
            corner.0,
            corner.1,
        ));
    }
    let last = vertices[vertices.len() - 1];
    commands.push(StepCommand::Line(last.0, last.1));
    commands
}

fn apply_step(builder: &mut PathBuilder, commands: &[StepCommand]) {
    for command in commands {
        match command {
            StepCommand::Line(x, y) => builder.line_to(point(px(*x), px(*y))),
            StepCommand::Curve(to_x, to_y, ctrl_x, ctrl_y) => {
                builder.curve_to(point(px(*to_x), px(*to_y)), point(px(*ctrl_x), px(*ctrl_y)));
            }
        }
    }
}

/// 本周期辅助线：50% 中线、「现在」竖线、理想配速对角参考线（中性色，不画伪采样圆点）。
fn paint_period_guides(
    bounds: Bounds<Pixels>,
    period: &HistoryPeriodView,
    theme: &Theme,
    window: &mut Window,
) {
    let plot = line_plot_bounds(bounds);
    // 50% 参考线画成和顶部弱参考线一样的细实线。
    let mid_y = plot.origin.y + plot.size.height / 2.0;
    let width = (bounds.origin.x + bounds.size.width - plot.origin.x).max(px(1.0));
    window.paint_quad(fill(
        Bounds {
            origin: point(plot.origin.x, mid_y - px(0.5)),
            size: size(width, px(1.0)),
        },
        theme.border.strong.opacity(0.4),
    ));

    // 「现在」竖线：1px 点线，从绘图区顶到底。
    let now_x = plot.origin.x + plot.size.width * (period.now_x_ratio as f32);
    let mut now_builder = PathBuilder::stroke(px(1.0)).dash_array(&[px(1.5), px(2.5)]);
    now_builder.move_to(point(now_x, plot.origin.y));
    now_builder.line_to(point(now_x, plot.origin.y + plot.size.height));
    if let Ok(path) = now_builder.build() {
        window.paint_path(path, theme.text.muted);
    }

    // 理想配速：从周期起点满额斜到重置时用完，用中性淡虚线作背景参考，避免与数据断档虚线撞色。
    let pace_from = chart_point(bounds, 0.0, period.pace_start_y_ratio);
    let pace_to = chart_point(bounds, 1.0, period.pace_end_y_ratio);
    let mut pace_builder = PathBuilder::stroke(px(PACE_STROKE)).dash_array(&[px(4.0), px(3.0)]);
    pace_builder.move_to(pace_from);
    pace_builder.line_to(pace_to);
    if let Ok(path) = pace_builder.build() {
        window.paint_path(path, theme.text.muted.opacity(PACE_OPACITY));
    }
}

fn line_plot_bounds(bounds: Bounds<Pixels>) -> Bounds<Pixels> {
    let left = px(LABEL_PAD + PopupLayout::CARD_HISTORY_Y_GUTTER);
    let inset_x = px(PLOT_INSET_X);
    let inset_y = px(PLOT_INSET_Y);
    Bounds {
        origin: point(bounds.origin.x + left, bounds.origin.y + inset_y),
        size: size(
            (bounds.size.width - left - inset_x).max(px(1.0)),
            (bounds.size.height - inset_y * 2.0).max(px(1.0)),
        ),
    }
}

fn chart_point(bounds: Bounds<Pixels>, x_ratio: f64, y_ratio: f64) -> Point<Pixels> {
    let plot = line_plot_bounds(bounds);
    point(
        plot.origin.x + plot.size.width * (x_ratio as f32),
        plot.origin.y + plot.size.height * (1.0 - y_ratio as f32),
    )
}

/// 绘图区画顶栏弱参考线与底边基线，锚定绘图框上下边界，避免持平折线上方像留白断层。
fn paint_baseline(bounds: Bounds<Pixels>, color: Hsla, window: &mut Window) {
    let plot = line_plot_bounds(bounds);
    let stroke = px(1.0);
    let right = bounds.origin.x + bounds.size.width;
    let width = (right - plot.origin.x).max(stroke);
    window.paint_quad(fill(
        Bounds {
            origin: point(plot.origin.x, plot.origin.y),
            size: size(width, stroke),
        },
        color.opacity(0.4),
    ));
    window.paint_quad(fill(
        Bounds {
            origin: point(plot.origin.x, plot.origin.y + plot.size.height - stroke),
            size: size(width, stroke),
        },
        color,
    ));
}

fn paint_hover_guides(
    bounds: Bounds<Pixels>,
    hit: &HistoryHoverHit,
    color: Hsla,
    window: &mut Window,
) {
    let plot = line_plot_bounds(bounds);
    let cross_x = plot.origin.x + plot.size.width * (hit.x_ratio as f32);
    // 指示线留在折线下面。盖在上面时，虚线会把蓝色笔画像抠出缺口。
    paint_dashed(
        point(cross_x, bounds.origin.y + px(1.0)),
        point(cross_x, bounds.origin.y + bounds.size.height - px(1.0)),
        color,
        window,
    );
    if let Some(y_ratio) = hit.y_ratio {
        let cross_y = plot.origin.y + plot.size.height * (1.0 - y_ratio as f32);
        paint_dashed(
            point(plot.origin.x, cross_y),
            point(plot.origin.x + plot.size.width, cross_y),
            color,
            window,
        );
    }
}

fn paint_hover_marker(
    bounds: Bounds<Pixels>,
    hit: &HistoryHoverHit,
    primary_override: Option<Hsla>,
    theme: &Theme,
    colors: HoverColors,
    window: &mut Window,
    cx: &mut App,
) {
    let plot = line_plot_bounds(bounds);
    let cross_x = plot.origin.x + plot.size.width * (hit.x_ratio as f32);
    let mut anchor_x = cross_x;
    let mut anchor_y = window.mouse_position().y - bounds.origin.y;
    for mark in &hit.marks {
        let mark_x = plot.origin.x + plot.size.width * (mark.x_ratio as f32);
        let cross_y = plot.origin.y + plot.size.height * (1.0 - mark.y_ratio as f32);
        let radius = px(2.5);
        window.paint_quad(
            fill(
                Bounds {
                    origin: point(mark_x - radius, cross_y - radius),
                    size: size(radius * 2.0, radius * 2.0),
                },
                resolve_line_color(mark.color_index, primary_override, theme),
            )
            .corner_radii(radius),
        );
    }
    // 多条线各自吸到自己的采样，没有同一个交点，读数块留在指针旁。
    if hit.marks.len() == 1 {
        let mark = &hit.marks[0];
        anchor_x = plot.origin.x + plot.size.width * (mark.x_ratio as f32);
        anchor_y = plot.origin.y + plot.size.height * (1.0 - mark.y_ratio as f32) - bounds.origin.y;
    }
    if !hit.rows.is_empty() {
        paint_hover_chip(
            bounds,
            anchor_x - bounds.origin.x,
            anchor_y,
            &hit.rows,
            colors,
            window,
            cx,
        );
    }
}

fn paint_dashed(from: Point<Pixels>, to: Point<Pixels>, color: Hsla, window: &mut Window) {
    let mut builder = PathBuilder::stroke(px(1.0)).dash_array(&[px(3.0), px(2.0)]);
    builder.move_to(from);
    builder.line_to(to);
    if let Ok(path) = builder.build() {
        window.paint_path(path, color);
    }
}

fn paint_hover_chip(
    bounds: Bounds<Pixels>,
    anchor_x: Pixels,
    anchor_y: Pixels,
    rows: &[String],
    colors: HoverColors,
    window: &mut Window,
    cx: &mut App,
) {
    if rows.is_empty() {
        return;
    }
    let mut style = window.text_style();
    style.color = colors.text;
    style.font_size = px(CHIP_FONT).into();
    style.background_color = None;
    let mut shaped = Vec::with_capacity(rows.len());
    let mut chip_w = 0.0f32;
    for row in rows {
        let label = SharedString::from(row.clone());
        let run = style.to_run(label.len());
        let line = window
            .text_system()
            .shape_line(label, px(CHIP_FONT), &[run], None);
        chip_w = chip_w.max(line.width / px(1.0));
        shaped.push(line);
    }
    let chip_w = chip_w + CHIP_PAD_X * 2.0;
    let chip_h = rows.len() as f32 * CHIP_LINE + CHIP_PAD_Y * 2.0;
    let (chip_x, chip_y) = hover_chip_origin(
        anchor_x / px(1.0),
        anchor_y / px(1.0),
        bounds.size.width / px(1.0),
        bounds.size.height / px(1.0),
        chip_w,
        chip_h,
    );
    let origin = point(bounds.origin.x + px(chip_x), bounds.origin.y + px(chip_y));
    window.paint_quad(quad(
        Bounds {
            origin,
            size: size(px(chip_w), px(chip_h)),
        },
        px(4.0),
        colors.chip_bg,
        px(1.0),
        colors.guide,
        BorderStyle::Solid,
    ));
    for (index, line) in shaped.into_iter().enumerate() {
        let _ = line.paint(
            point(
                origin.x + px(CHIP_PAD_X),
                origin.y + px(CHIP_PAD_Y + index as f32 * CHIP_LINE),
            ),
            px(CHIP_LINE),
            window,
            cx,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(x: f32, y: f32, value: f64) -> SamplePoint {
        SamplePoint { x, y, value }
    }

    #[test]
    fn plateau_holds_until_the_value_changes() {
        let flat = plateau_vertices(&[
            sample(0.0, 10.0, 70.0),
            sample(15.0, 10.0, 70.0),
            sample(30.0, 10.0, 70.0),
        ]);
        assert_eq!(flat, vec![(0.0, 10.0), (30.0, 10.0)]);

        let step = plateau_vertices(&[
            sample(0.0, 10.0, 70.0),
            sample(15.0, 10.0, 70.0),
            sample(30.0, 4.0, 60.0),
            sample(45.0, 4.0, 60.0),
        ]);
        assert_eq!(
            step,
            vec![(0.0, 10.0), (30.0, 10.0), (30.0, 4.0), (45.0, 4.0)]
        );
    }

    #[test]
    fn rounded_step_cuts_the_corner_and_shrinks_on_a_short_riser() {
        let wide = rounded_step(&[(0.0, 10.0), (100.0, 10.0), (100.0, 40.0)], 4.0);
        assert_eq!(
            wide,
            vec![
                StepCommand::Line(96.0, 10.0),
                StepCommand::Curve(100.0, 14.0, 100.0, 10.0),
                StepCommand::Line(100.0, 40.0),
            ]
        );

        let short = rounded_step(&[(0.0, 0.0), (20.0, 0.0), (20.0, 4.0), (50.0, 4.0)], 4.0);
        assert_eq!(
            short,
            vec![
                StepCommand::Line(18.0, 0.0),
                StepCommand::Curve(20.0, 2.0, 20.0, 0.0),
                StepCommand::Line(20.0, 2.0),
                StepCommand::Curve(22.0, 4.0, 20.0, 4.0),
                StepCommand::Line(50.0, 4.0),
            ]
        );
    }

    #[test]
    fn chart_fill_opacity_attenuates_flat_series_and_disables_on_multi_or_period() {
        assert_eq!(chart_fill_opacity(false, 1, false), 0.0);
        assert_eq!(chart_fill_opacity(true, 2, false), 0.0);
        assert!((chart_fill_opacity(true, 1, false) - STEP_FILL).abs() < 1e-6);
        assert!((chart_fill_opacity(true, 1, true) - STEP_FILL * FLAT_FILL_FACTOR).abs() < 1e-6);
    }
}
