//! 折线视图构建：`HistoryReady` → 归一化坐标的 `HistoryChartView`。
//! 设置页与托盘弹窗共用同一套聚合结果，只在外层布局上不同。
//! 横轴按样本时间展开，纵轴按样本最小、最大值留白。选中窗口只用来筛点。
//! 计量配额的纵轴跟着额度显示：剩余是 `limit - used`，已用是样本里的 used。纯余额没有已用，始终画剩余。

use super::format::format_quota_label_spec;
use super::{
    HistoryChartAxisView, HistoryChartView, HistoryLineView, HistoryPointView, HistorySegmentView,
};
use crate::history::{
    is_line_series, plot_on_scale, plot_segment, series_x_bounds, shared_y_scale, HistoryReady,
    HistoryReadyState, HistorySegment, HistorySeries, HistoryUnit, HistoryYKind, SharedYScale,
};
use crate::models::{QuotaDisplayMode, QuotaLabelSpec};
use chrono::{Datelike, Local, Timelike};

/// 非 Charts 状态（空、全失败、无数值）统一返回空列表，由 UI 决定要不要占位。
/// 少于 2 个点的序列画不出折线，只剩角落一个点，直接过滤。
/// 每条配额各自一张图。托盘和设置页都用这个。
pub fn history_charts_view(ready: &HistoryReady, mode: QuotaDisplayMode) -> Vec<HistoryChartView> {
    build_history_charts(ready, mode)
}

fn build_history_charts(ready: &HistoryReady, mode: QuotaDisplayMode) -> Vec<HistoryChartView> {
    let HistoryReadyState::Charts(series) = &ready.state else {
        return Vec::new();
    };
    let projected: Vec<HistorySeries> = series
        .iter()
        .filter(|item| is_line_series(item))
        .map(|item| project_display_series(item, mode))
        .collect();
    let groups: Vec<Vec<&HistorySeries>> = projected.iter().map(|item| vec![item]).collect();
    groups
        .iter()
        .map(|group| history_chart_view(group, ready.axis_start_ms, ready.axis_end_ms))
        .collect()
}

/// 历史线程不读设置。剩余只在视图里由已用和 limit 换算。
fn project_display_series(series: &HistorySeries, mode: QuotaDisplayMode) -> HistorySeries {
    if mode != QuotaDisplayMode::Remaining {
        return series.clone();
    }
    let mut projected = series.clone();
    for segment in &mut projected.segments {
        if segment.y_kind != HistoryYKind::MeteredUsed {
            continue;
        }
        if segment.points.iter().any(|point| point.limit.is_none()) {
            continue;
        }
        for point in &mut segment.points {
            if let Some(limit) = point.limit {
                point.y = limit - point.y;
            }
        }
        segment.y_kind = HistoryYKind::BalanceRemaining;
    }
    projected
}

fn history_chart_view(
    series: &[&HistorySeries],
    axis_start_ms: i64,
    axis_end_ms: i64,
) -> HistoryChartView {
    let owned: Vec<HistorySegment> = series
        .iter()
        .flat_map(|item| item.segments.clone())
        .collect();
    let scale = shared_y_scale(&owned);
    let (x_start_ms, x_end_ms) = series_x_bounds(&owned)
        .map(|bounds| (bounds.start_ms, bounds.end_ms))
        .unwrap_or((axis_start_ms, axis_end_ms));
    // 留白只挪开点和边框。两端文字用第一个和最后一个样本，避免贴近日界的图被标成跨天。
    let (label_start_ms, label_end_ms) =
        sample_time_extent(&owned).unwrap_or((x_start_ms, x_end_ms));
    let lines = series
        .iter()
        .enumerate()
        .map(|(index, item)| HistoryLineView {
            quota_key: item.quota_key.clone(),
            title: history_series_title(&item.label_spec_json),
            color_index: index,
            segments: item
                .segments
                .iter()
                .map(|segment| {
                    let points = if let Some(scale) = &scale {
                        plot_on_scale(segment, x_start_ms, x_end_ms, scale.min, scale.max)
                    } else {
                        plot_segment(segment, x_start_ms, x_end_ms).1
                    };
                    debug_assert_eq!(points.len(), segment.points.len());
                    HistorySegmentView {
                        unit: segment.unit,
                        points: segment
                            .points
                            .iter()
                            .zip(points)
                            .map(|(source, point)| HistoryPointView {
                                at_ms: source.bucket_start_ms,
                                y: source.y,
                                x_ratio: point.x_ratio,
                                y_ratio: point.y_ratio,
                                gap_before: source.gap_before,
                            })
                            .collect(),
                    }
                })
                .collect(),
        })
        .collect();
    let title = if series.len() == 1 {
        history_series_title(&series[0].label_spec_json)
    } else {
        String::new()
    };
    let quota_key = if series.len() == 1 {
        series[0].quota_key.clone()
    } else {
        series
            .iter()
            .map(|item| item.quota_key.as_str())
            .collect::<Vec<_>>()
            .join("|")
    };
    HistoryChartView {
        quota_key,
        title,
        lines,
        axis: history_axis_view(
            scale.as_ref(),
            x_start_ms,
            x_end_ms,
            label_start_ms,
            label_end_ms,
        ),
    }
}

fn sample_time_extent(segments: &[HistorySegment]) -> Option<(i64, i64)> {
    let mut min = i64::MAX;
    let mut max = i64::MIN;
    let mut any = false;
    for segment in segments {
        for point in &segment.points {
            any = true;
            min = min.min(point.bucket_start_ms);
            max = max.max(point.bucket_start_ms);
        }
    }
    any.then_some((min, max))
}

fn history_axis_view(
    scale: Option<&SharedYScale>,
    axis_start_ms: i64,
    axis_end_ms: i64,
    label_start_ms: i64,
    label_end_ms: i64,
) -> HistoryChartAxisView {
    let (y_label, y_max, y_min) = match scale {
        Some(scale) => {
            let (y_max, y_min) = format_axis_ends(scale.min, scale.max, scale.unit);
            (history_y_label(scale.y_kind), y_max, y_min)
        }
        None => (String::new(), String::new(), String::new()),
    };
    let (x_start, x_end) = format_history_axis_times(label_start_ms, label_end_ms);
    HistoryChartAxisView {
        y_label,
        y_max,
        y_min,
        x_start,
        x_end,
        x_start_ms: axis_start_ms,
        x_end_ms: axis_end_ms,
    }
}

/// 指针在折线区域里的读数。`x_ratio` 从左到右，0 和 1 是留白后横轴的两端。
///
/// 读数吸到最近的真实采样，时间和数值都是那一次采样的，不在两次采样之间插值。
/// `hit_slop_ms` 是圆点在时间上的命中半径。孤立采样靠它才能被点中；半径再被相邻采样的中点截断。
/// 断档中段和留白远端只有时间，不编一个值。
#[derive(Debug, Clone, PartialEq)]
pub struct HistoryHoverMark {
    pub text: String,
    pub x_ratio: f64,
    pub y_ratio: f64,
    pub color_index: usize,
}

pub struct HistoryHoverHit {
    pub label: String,
    pub x_ratio: f64,
    pub y_ratio: Option<f64>,
    /// 读数块的每一行。一条折线时就是 `label`；多条时第一行是时间，后面每条配额一行。
    pub rows: Vec<String>,
    pub marks: Vec<HistoryHoverMark>,
}

pub fn history_hover_hit(
    segments: &[HistorySegmentView],
    x_start_ms: i64,
    x_end_ms: i64,
    x_ratio: f64,
    hit_slop_ms: f64,
) -> HistoryHoverHit {
    let line = HistoryLineView {
        quota_key: String::new(),
        title: String::new(),
        color_index: 0,
        segments: segments.to_vec(),
    };
    history_hover_lines(
        std::slice::from_ref(&line),
        x_start_ms,
        x_end_ms,
        x_ratio,
        hit_slop_ms,
    )
}

pub fn history_hover_lines(
    lines: &[HistoryLineView],
    x_start_ms: i64,
    x_end_ms: i64,
    x_ratio: f64,
    hit_slop_ms: f64,
) -> HistoryHoverHit {
    let x_ratio = x_ratio.clamp(0.0, 1.0);
    let span = (x_end_ms - x_start_ms).max(1) as f64;
    let at = x_start_ms as f64 + x_ratio * span;
    let time_only = plotted_sample_span(lines)
        .map(|(start, end)| axis_time_only(start, end))
        .unwrap_or_else(|| axis_time_only(x_start_ms, x_end_ms));
    let cursor_time = axis_clock(at.round() as i64)
        .map(|clock| format_history_axis_clock(clock, time_only))
        .unwrap_or_default();
    let name_lines = lines.len() > 1;
    let marks = lines
        .iter()
        .filter_map(|line| {
            let sample = sample_at(&line.segments, at, hit_slop_ms)?;
            let value = format_axis_value(sample.y, sample.unit, axis_decimals(sample.unit));
            let sample_time = axis_clock(sample.at_ms)
                .map(|clock| format_history_axis_clock(clock, time_only))
                .unwrap_or_default();
            let body = if name_lines {
                format!("{}  {value}", line.title)
            } else {
                value
            };
            Some(HistoryHoverMark {
                text: join_hover_time(&sample_time, &body),
                x_ratio: sample.x_ratio,
                y_ratio: sample.y_ratio,
                color_index: line.color_index,
            })
        })
        .collect::<Vec<_>>();
    let y_ratio = (marks.len() == 1).then(|| marks[0].y_ratio);
    let x_ratio = if marks.len() == 1 {
        marks[0].x_ratio
    } else {
        x_ratio
    };
    let label = if marks.len() == 1 {
        marks[0].text.clone()
    } else {
        cursor_time
    };
    let rows = if marks.is_empty() {
        if label.is_empty() {
            Vec::new()
        } else {
            vec![label.clone()]
        }
    } else {
        marks.iter().map(|mark| mark.text.clone()).collect()
    };
    HistoryHoverHit {
        label,
        x_ratio,
        y_ratio,
        rows,
        marks,
    }
}

fn join_hover_time(time: &str, text: &str) -> String {
    if time.is_empty() {
        text.to_string()
    } else {
        format!("{time}  {text}")
    }
}

/// 读数块相对绘图框左上角的位置。默认在交点右上方，靠近右缘或上缘时翻到另一侧。
pub fn hover_chip_origin(
    anchor_x: f32,
    anchor_y: f32,
    frame_w: f32,
    frame_h: f32,
    chip_w: f32,
    chip_h: f32,
) -> (f32, f32) {
    const GAP: f32 = 6.0;
    const PAD: f32 = 4.0;
    let mut x = anchor_x + GAP;
    let mut y = anchor_y - GAP - chip_h;
    if x + chip_w > frame_w - PAD {
        x = anchor_x - GAP - chip_w;
    }
    if y < PAD {
        y = anchor_y + GAP;
    }
    let max_x = (frame_w - chip_w - PAD).max(PAD);
    let max_y = (frame_h - chip_h - PAD).max(PAD);
    (x.clamp(PAD, max_x), y.clamp(PAD, max_y))
}

struct HoverSample {
    at_ms: i64,
    y: f64,
    x_ratio: f64,
    y_ratio: f64,
    unit: HistoryUnit,
}

fn plotted_sample_span(lines: &[HistoryLineView]) -> Option<(i64, i64)> {
    let mut min = i64::MAX;
    let mut max = i64::MIN;
    let mut any = false;
    for line in lines {
        for segment in &line.segments {
            for point in &segment.points {
                any = true;
                min = min.min(point.at_ms);
                max = max.max(point.at_ms);
            }
        }
    }
    any.then_some((min, max))
}

/// 连续采样的整段都可读，吸到最近的一次，时间正中留在较早的采样。
/// 断档把采样拆成单独的点。这种点如果只认那一个瞬间，鼠标点不中，所以向外放 `hit_slop_ms`。
/// 向外不超过相邻采样的中点，断档中段仍然只有时间。
fn sample_at(segments: &[HistorySegmentView], at: f64, hit_slop_ms: f64) -> Option<HoverSample> {
    let hit_slop_ms = hit_slop_ms.max(0.0);
    let mut runs: Vec<Vec<(&HistoryPointView, HistoryUnit)>> = Vec::new();
    for segment in segments {
        for point in &segment.points {
            if point.gap_before || runs.is_empty() {
                runs.push(vec![(point, segment.unit)]);
            } else {
                runs.last_mut().unwrap().push((point, segment.unit));
            }
        }
    }
    for run in &runs {
        let first = run[0].0.at_ms as f64;
        let last = run[run.len() - 1].0.at_ms as f64;
        if at >= first - 0.5 && at <= last + 0.5 {
            return Some(nearest_in_run(run, at));
        }
    }
    let flat: Vec<(&HistoryPointView, HistoryUnit)> = runs.into_iter().flatten().collect();
    let mut best: Option<(usize, f64)> = None;
    for (index, (point, _)) in flat.iter().enumerate() {
        let delta = at - point.at_ms as f64;
        let neighbor = if delta < 0.0 {
            index.checked_sub(1)
        } else {
            Some(index + 1).filter(|next| *next < flat.len())
        };
        let limit = outward_limit(
            neighbor
                .and_then(|slot| flat.get(slot))
                .map(|(near, _)| near.at_ms),
            point.at_ms,
            hit_slop_ms,
        );
        let dist = delta.abs();
        if dist < limit && best.is_none_or(|(_, best_dist)| dist < best_dist) {
            best = Some((index, dist));
        }
    }
    best.map(|(index, _)| {
        let (point, unit) = flat[index];
        hover_sample(point, unit)
    })
}

fn nearest_in_run(run: &[(&HistoryPointView, HistoryUnit)], at: f64) -> HoverSample {
    let mut best = 0;
    let mut best_dist = f64::MAX;
    for (index, (point, _)) in run.iter().enumerate() {
        let dist = (at - point.at_ms as f64).abs();
        if dist < best_dist {
            best = index;
            best_dist = dist;
        }
    }
    let (point, unit) = run[best];
    hover_sample(point, unit)
}

fn hover_sample(point: &HistoryPointView, unit: HistoryUnit) -> HoverSample {
    HoverSample {
        at_ms: point.at_ms,
        y: point.y,
        x_ratio: point.x_ratio,
        y_ratio: point.y_ratio,
        unit,
    }
}

/// 没有相邻采样时用完整半径。有相邻采样时停在中点外侧，中点本身不算命中。
fn outward_limit(neighbor_ms: Option<i64>, at_ms: i64, hit_slop_ms: f64) -> f64 {
    let Some(neighbor_ms) = neighbor_ms else {
        return hit_slop_ms;
    };
    let half = (at_ms - neighbor_ms).abs() as f64 / 2.0;
    half.min(hit_slop_ms)
}

fn axis_decimals(unit: HistoryUnit) -> usize {
    match unit {
        HistoryUnit::Percentage | HistoryUnit::Amount => 1,
        HistoryUnit::Currency => 2,
    }
}

fn axis_time_only(start_ms: i64, end_ms: i64) -> bool {
    match (axis_clock(start_ms), axis_clock(end_ms)) {
        (Some(start), Some(end)) => start.month == end.month && start.day == end.day,
        _ => false,
    }
}

fn history_y_label(kind: HistoryYKind) -> String {
    match kind {
        HistoryYKind::MeteredUsed => rust_i18n::t!("provider.history.axis.used").to_string(),
        HistoryYKind::BalanceRemaining => {
            rust_i18n::t!("provider.history.axis.remaining").to_string()
        }
    }
}

/// 持平只标一个值，`y_min` 留空。百分比先取整。两端写成同一个字符串时再多留一位，避免 0% 和 0% 并排。
fn format_axis_ends(min: f64, max: f64, unit: HistoryUnit) -> (String, String) {
    let base = match unit {
        HistoryUnit::Percentage => 0,
        HistoryUnit::Amount => 1,
        HistoryUnit::Currency => 2,
    };
    if (max - min).abs() < 1e-9 {
        return (format_axis_value(max, unit, base), String::new());
    }
    let mut decimals = base;
    loop {
        let lo = format_axis_value(min, unit, decimals);
        let hi = format_axis_value(max, unit, decimals);
        if lo != hi || decimals >= 4 {
            return (hi, lo);
        }
        decimals += 1;
    }
}

fn format_axis_value(value: f64, unit: HistoryUnit, decimals: usize) -> String {
    let number = format_axis_number(value, decimals);
    match unit {
        HistoryUnit::Percentage => format!("{number}%"),
        HistoryUnit::Currency => format!("${number}"),
        HistoryUnit::Amount => number,
    }
}

fn format_axis_number(value: f64, decimals: usize) -> String {
    let text = format!("{value:.prec$}", prec = decimals);
    // 只去掉小数末尾的 0。整数末尾的 0 是有效位，50 不能收成 5。
    let trimmed = match text.split_once('.') {
        Some((whole, frac)) => {
            let frac = frac.trim_end_matches('0');
            if frac.is_empty() {
                whole.to_string()
            } else {
                format!("{whole}.{frac}")
            }
        }
        None => text,
    };
    if trimmed.is_empty() || trimmed == "-" || trimmed == "-0" {
        "0".to_string()
    } else {
        trimmed
    }
}

fn format_history_axis_times(start_ms: i64, end_ms: i64) -> (String, String) {
    let start = axis_clock(start_ms);
    let end = axis_clock(end_ms);
    let time_only = axis_time_only(start_ms, end_ms);
    (
        start
            .map(|clock| format_history_axis_clock(clock, time_only))
            .unwrap_or_default(),
        end.map(|clock| format_history_axis_clock(clock, time_only))
            .unwrap_or_default(),
    )
}

#[derive(Clone, Copy)]
struct AxisClock {
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
}

fn axis_clock(ms: i64) -> Option<AxisClock> {
    let utc = chrono::DateTime::from_timestamp_millis(ms)?;
    let local = utc.with_timezone(&Local);
    Some(AxisClock {
        month: local.month(),
        day: local.day(),
        hour: local.hour(),
        minute: local.minute(),
    })
}

/// 同一天只标时分。跨天用 `10/4 19:51`，不再写「月」「日」。
fn format_history_axis_clock(clock: AxisClock, time_only: bool) -> String {
    if time_only {
        format!("{:02}:{:02}", clock.hour, clock.minute)
    } else {
        format!(
            "{}/{} {:02}:{:02}",
            clock.month, clock.day, clock.hour, clock.minute
        )
    }
}

fn history_series_title(label_spec_json: &str) -> String {
    let label_spec = serde_json::from_str::<QuotaLabelSpec>(label_spec_json)
        .unwrap_or_else(|_| QuotaLabelSpec::Raw(label_spec_json.to_string()));
    format_quota_label_spec(&label_spec)
}

#[cfg(test)]
mod tests {
    use super::super::HistoryLineView;
    use super::*;
    use crate::history::{
        HistoryRange, HistoryReady, HistoryReadyState, HistorySegment, HistorySeries, HistoryUnit,
        HistoryYKind, SeriesPoint,
    };

    fn series(key: &str, point_count: usize) -> HistorySeries {
        HistorySeries {
            quota_key: key.to_string(),
            label_spec_json: "{\"Raw\":\"quota\"}".to_string(),
            segments: vec![HistorySegment {
                unit: HistoryUnit::Percentage,
                y_kind: HistoryYKind::MeteredUsed,
                points: (0..point_count)
                    .map(|i| SeriesPoint {
                        bucket_start_ms: i as i64 * 900_000,
                        y: 50.0,
                        limit: Some(100.0),
                        gap_before: false,
                    })
                    .collect(),
            }],
        }
    }

    fn ready(series: Vec<HistorySeries>) -> HistoryReady {
        HistoryReady {
            range: HistoryRange::Last24Hours,
            axis_start_ms: 0,
            axis_end_ms: 86_400_000,
            state: HistoryReadyState::Charts(series),
        }
    }

    #[test]
    fn single_point_series_are_filtered_out() {
        let charts = history_charts_view(
            &ready(vec![series("a", 1), series("b", 3)]),
            QuotaDisplayMode::Used,
        );
        assert_eq!(charts.len(), 1);
        assert_eq!(charts[0].quota_key, "b");

        let charts = history_charts_view(&ready(vec![series("a", 1)]), QuotaDisplayMode::Used);
        assert!(charts.is_empty());
    }

    #[test]
    fn flat_percentage_axis_labels_the_sample_not_zero_to_hundred() {
        let _locale = crate::i18n::test_locale_guard("zh-CN");
        let charts =
            history_charts_view(&ready(vec![series("session", 3)]), QuotaDisplayMode::Used);
        let chart = &charts[0];
        assert_eq!(chart.title, "quota");
        assert_eq!(chart.axis.y_label, "已用");
        assert_eq!(chart.axis.y_max, "50%");
        assert!(chart.axis.y_min.is_empty());
        assert!(!chart.axis.x_start.is_empty());
        assert_ne!(chart.axis.x_start, chart.axis.x_end);
        let points = &chart.lines[0].segments[0].points;
        assert!((points[0].y_ratio - 0.5).abs() < 1e-9);
        assert!(points[0].x_ratio < 0.2);
        assert!(points[2].x_ratio > 0.8);
    }

    #[test]
    fn percentage_axis_rounds_to_whole_numbers() {
        let mut item = series("weekly", 2);
        item.segments[0].points[0].y = 50.0;
        item.segments[0].points[1].y = 70.0;
        let chart = &history_charts_view(&ready(vec![item]), QuotaDisplayMode::Used)[0];
        assert_eq!(chart.axis.y_min, "47%");
        assert_eq!(chart.axis.y_max, "73%");
    }

    #[test]
    fn percentage_axis_keeps_a_decimal_when_whole_numbers_match() {
        let mut item = series("weekly", 2);
        item.segments[0].points[0].y = 66.2;
        item.segments[0].points[1].y = 66.4;
        let chart = &history_charts_view(&ready(vec![item]), QuotaDisplayMode::Used)[0];
        assert_eq!(chart.axis.y_min, "66.2%");
        assert_eq!(chart.axis.y_max, "66.4%");
    }

    #[test]
    fn axis_time_is_compact() {
        let same_day = AxisClock {
            month: 10,
            day: 4,
            hour: 19,
            minute: 51,
        };
        let later = AxisClock {
            month: 10,
            day: 4,
            hour: 22,
            minute: 23,
        };
        assert_eq!(format_history_axis_clock(same_day, true), "19:51");
        assert_eq!(format_history_axis_clock(later, true), "22:23");
        assert_eq!(format_history_axis_clock(same_day, false), "10/4 19:51");
    }

    #[test]
    fn balance_axis_uses_remaining_min_and_max() {
        let _locale = crate::i18n::test_locale_guard("en");
        let mut item = series("credit", 2);
        item.segments[0].unit = HistoryUnit::Amount;
        item.segments[0].y_kind = HistoryYKind::BalanceRemaining;
        item.segments[0].points[0].y = 10.0;
        item.segments[0].points[1].y = 30.0;
        let chart = &history_charts_view(&ready(vec![item]), QuotaDisplayMode::Used)[0];
        assert_eq!(chart.axis.y_label, "Remaining");
        let pad = 20.0 * 0.15;
        let low = 10.0 - pad;
        let high = 30.0 + pad;
        assert_eq!(chart.axis.y_min, "7");
        assert_eq!(chart.axis.y_max, "33");
        let points = &chart.lines[0].segments[0].points;
        assert!((points[0].y_ratio - ((10.0 - low) / (high - low))).abs() < 1e-9);
        assert!((points[1].y_ratio - ((30.0 - low) / (high - low))).abs() < 1e-9);
        assert!(points[1].y_ratio > points[0].y_ratio);
        assert!(points[0].x_ratio < 0.2);
        assert!(points[1].x_ratio > 0.8);
    }

    #[test]
    fn remaining_mode_plots_limit_minus_used() {
        let _locale = crate::i18n::test_locale_guard("zh-CN");
        let mut item = series("weekly", 2);
        item.segments[0].points[0].y = 29.1;
        item.segments[0].points[1].y = 36.9;
        let remaining =
            &history_charts_view(&ready(vec![item.clone()]), QuotaDisplayMode::Remaining)[0];
        assert_eq!(remaining.axis.y_label, "剩余");
        assert!((remaining.lines[0].segments[0].points[0].y - 70.9).abs() < 1e-9);
        assert!(
            remaining.lines[0].segments[0].points[0].y_ratio
                > remaining.lines[0].segments[0].points[1].y_ratio
        );

        let used = &history_charts_view(&ready(vec![item]), QuotaDisplayMode::Used)[0];
        assert_eq!(used.axis.y_label, "已用");
        assert!((used.lines[0].segments[0].points[0].y - 29.1).abs() < 1e-9);
        assert!(
            used.lines[0].segments[0].points[0].y_ratio
                < used.lines[0].segments[0].points[1].y_ratio
        );
    }

    fn hover_point(at_ms: i64, y: f64, y_ratio: f64, gap_before: bool) -> HistoryPointView {
        HistoryPointView {
            at_ms,
            y,
            x_ratio: 0.0,
            y_ratio,
            gap_before,
        }
    }

    #[test]
    fn hover_snaps_to_a_real_sample_and_skips_gaps() {
        let segments = vec![HistorySegmentView {
            unit: HistoryUnit::Percentage,
            points: vec![
                hover_point(1_000, 10.0, 0.2, false),
                hover_point(5_000, 30.0, 0.8, false),
                hover_point(9_000, 90.0, 0.1, true),
            ],
        }];
        // 1000 与 5000 的正中是 3000。不插值成 20%，留在较早的那次采样。
        let mid = history_hover_hit(&segments, 1_000, 9_000, 0.25, 0.5);
        let early = format_history_axis_clock(axis_clock(1_000).unwrap(), true);
        assert_eq!(mid.label, format!("{early}  10%"));
        assert!((mid.y_ratio.unwrap() - 0.2).abs() < 1e-9);

        let later = history_hover_hit(&segments, 1_000, 9_000, 0.375, 0.5);
        let later_time = format_history_axis_clock(axis_clock(5_000).unwrap(), true);
        assert_eq!(later.label, format!("{later_time}  30%"));
        assert!((later.y_ratio.unwrap() - 0.8).abs() < 1e-9);

        let on_start = history_hover_hit(&segments, 1_000, 9_000, 0.0, 0.5);
        assert!(on_start.label.ends_with("  10%"));
        assert!((on_start.y_ratio.unwrap() - 0.2).abs() < 1e-9);

        let gap = history_hover_hit(&segments, 1_000, 9_000, 0.75, 0.5);
        let gap_time = format_history_axis_clock(axis_clock(7_000).unwrap(), true);
        assert_eq!(gap.label, gap_time);
        assert!(gap.y_ratio.is_none());

        let before = history_hover_hit(&segments, 0, 9_000, 0.0, 0.5);
        assert!(before.y_ratio.is_none());
        assert!(!before.label.contains('%'));

        // 样本本身在同一天。轴拉得很宽也不给读数加月日。
        let wide_axis = history_hover_hit(&segments, 0, 86_400_000, 0.0, 0.5);
        assert!(!wide_axis.label.contains('/'));
        assert!(wide_axis.y_ratio.is_none());
    }

    #[test]
    fn hover_reads_an_isolated_sample_near_its_dot_but_not_the_gap_middle() {
        let segments = vec![HistorySegmentView {
            unit: HistoryUnit::Percentage,
            points: vec![
                hover_point(0, 10.0, 0.2, false),
                hover_point(1_800_000, 40.0, 0.6, true),
            ],
        }];
        let axis_end = 86_400_000_i64;
        // 约 260px 宽的 24 小时图上，4px 比半个 30 分钟间隔更宽，命中仍停在中点外侧。
        let slop = axis_end as f64 / 260.0 * 4.0;
        let near = 1_800_000.0 - 5.0 * 60_000.0;
        let hit = history_hover_hit(&segments, 0, axis_end, near / axis_end as f64, slop);
        let time = format_history_axis_clock(axis_clock(1_800_000).unwrap(), true);
        assert_eq!(hit.label, format!("{time}  40%"));
        assert!(hit.y_ratio.is_some());

        let middle = history_hover_hit(&segments, 0, axis_end, 900_000.0 / axis_end as f64, slop);
        assert!(middle.y_ratio.is_none());
        assert!(!middle.label.contains('%'));
    }

    #[test]
    fn axis_text_uses_sample_times_when_padding_crosses_midnight() {
        let start = local_ms(2026, 6, 15, 0, 20).expect("local time");
        let end = local_ms(2026, 6, 15, 12, 0).expect("local time");
        let mut item = series("quota", 2);
        item.segments[0].points[0].bucket_start_ms = start;
        item.segments[0].points[1].bucket_start_ms = end;
        let chart = &history_charts_view(&ready(vec![item]), QuotaDisplayMode::Used)[0];
        let start_label = format_history_axis_clock(axis_clock(start).unwrap(), true);
        let end_label = format_history_axis_clock(axis_clock(end).unwrap(), true);
        assert_eq!(chart.axis.x_start, start_label);
        assert_eq!(chart.axis.x_end, end_label);
        assert!(!chart.axis.x_start.contains('/'));
        assert!(chart.axis.x_start_ms < start);
        assert!(chart.axis.x_end_ms > end);

        let padded = history_hover_hit(
            &chart.lines[0].segments,
            chart.axis.x_start_ms,
            chart.axis.x_end_ms,
            0.0,
            0.5,
        );
        assert!(padded.y_ratio.is_none());
        assert!(!padded.label.contains('/'));
    }

    #[test]
    fn axis_text_includes_the_date_when_samples_cross_midnight() {
        let start = local_ms(2026, 6, 15, 22, 0).expect("local time");
        let end = local_ms(2026, 6, 16, 2, 0).expect("local time");
        let mut item = series("quota", 2);
        item.segments[0].points[0].bucket_start_ms = start;
        item.segments[0].points[1].bucket_start_ms = end;
        let chart = &history_charts_view(&ready(vec![item]), QuotaDisplayMode::Used)[0];
        assert!(chart.axis.x_start.contains('/'));
        assert!(chart.axis.x_end.contains('/'));

        let hit = history_hover_hit(&chart.lines[0].segments, start, end, 0.0, 0.5);
        assert!(hit.y_ratio.is_some());
        assert!(hit.label.contains('/'));
    }

    fn local_ms(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> Option<i64> {
        use chrono::{LocalResult, NaiveDate, TimeZone};
        let naive = NaiveDate::from_ymd_opt(year, month, day)?.and_hms_opt(hour, minute, 0)?;
        match Local.from_local_datetime(&naive) {
            LocalResult::Single(time) => Some(time.timestamp_millis()),
            _ => None,
        }
    }

    #[test]
    fn hover_chip_flips_to_stay_inside_the_frame() {
        let (x, y) = hover_chip_origin(40.0, 40.0, 200.0, 80.0, 50.0, 16.0);
        assert!(x > 40.0);
        assert!(y < 40.0);

        let (x, y) = hover_chip_origin(180.0, 40.0, 200.0, 80.0, 50.0, 16.0);
        assert!(x < 180.0);
        assert!(x >= 4.0 && x + 50.0 <= 196.0);
        assert!(y >= 4.0 && y + 16.0 <= 76.0);

        let (x, y) = hover_chip_origin(40.0, 4.0, 200.0, 80.0, 50.0, 16.0);
        assert!(y > 4.0);
        assert!(x >= 4.0 && y + 16.0 <= 76.0);
    }

    #[test]
    fn merged_hover_names_each_line_and_skips_a_gap() {
        let mut lines = vec![
            HistoryLineView {
                quota_key: "daily".to_string(),
                title: "日配额".to_string(),
                color_index: 0,
                segments: vec![HistorySegmentView {
                    unit: HistoryUnit::Percentage,
                    points: vec![
                        hover_point(1_000, 10.0, 0.2, false),
                        hover_point(5_000, 30.0, 0.8, false),
                    ],
                }],
            },
            HistoryLineView {
                quota_key: "weekly".to_string(),
                title: "周配额".to_string(),
                color_index: 1,
                segments: vec![HistorySegmentView {
                    unit: HistoryUnit::Percentage,
                    points: vec![
                        hover_point(1_000, 80.0, 0.9, false),
                        hover_point(5_000, 80.0, 0.9, true),
                    ],
                }],
            },
        ];
        let one = history_hover_lines(&lines, 1_000, 5_000, 0.5, 0.5);
        assert!(one.label.contains("日配额"));
        assert!(one.label.contains("10%"));
        assert!(!one.label.contains("20%"));
        assert!(!one.label.contains("周配额"));
        assert!(one.y_ratio.is_some());

        lines[1].segments[0].points[1].gap_before = false;
        let both = history_hover_lines(&lines, 1_000, 5_000, 0.5, 0.5);
        assert!(both.y_ratio.is_none());
        assert_eq!(both.marks.len(), 2);
        assert!(both
            .rows
            .iter()
            .any(|row| row.contains("日配额") && row.contains("10%")));
        assert!(both
            .rows
            .iter()
            .any(|row| row.contains("周配额") && row.contains("80%")));
    }
}
