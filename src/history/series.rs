use super::sample::{HistoryStatus, HistoryUnit, HistoryValueKind};
use super::store::{HistoryPointRow, HistoryRow};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryRange {
    Last24Hours,
    Last7Days,
    Last30Days,
}

impl HistoryRange {
    pub fn window(self, now_ms: i64) -> (i64, i64) {
        let width = match self {
            Self::Last24Hours => 24 * 60 * 60 * 1000,
            Self::Last7Days => 7 * super::DAY_MS,
            Self::Last30Days => 30 * super::DAY_MS,
        };
        (now_ms.saturating_sub(width), now_ms)
    }

    fn bucket_width_ms(self) -> i64 {
        match self {
            Self::Last24Hours => 15 * 60 * 1000,
            Self::Last7Days => 60 * 60 * 1000,
            Self::Last30Days => 6 * 60 * 60 * 1000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryYKind {
    MeteredUsed,
    BalanceRemaining,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SeriesPoint {
    pub bucket_start_ms: i64,
    /// 计量配额这里是已用。剩余由视图按 `limit - y` 投影，不在历史线程里读设置。
    pub y: f64,
    pub limit: Option<f64>,
    pub gap_before: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HistorySegment {
    pub unit: HistoryUnit,
    pub y_kind: HistoryYKind,
    pub points: Vec<SeriesPoint>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HistorySeries {
    pub quota_key: String,
    pub label_spec_json: String,
    pub segments: Vec<HistorySegment>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HistoryReadyState {
    Empty,
    OnlyFailures { count: u64 },
    NoNumeric,
    Charts(Vec<HistorySeries>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct HistoryReady {
    pub range: HistoryRange,
    pub axis_start_ms: i64,
    pub axis_end_ms: i64,
    pub state: HistoryReadyState,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HistoryLoadOutcome {
    Ready(HistoryReady),
    Unavailable,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlottedPoint {
    pub x_ratio: f64,
    pub y_ratio: f64,
    pub gap_before: bool,
}

/// 少于两个点只能画成一个角落的点，不当作折线。
pub fn is_line_series(series: &HistorySeries) -> bool {
    series
        .segments
        .iter()
        .map(|segment| segment.points.len())
        .sum::<usize>()
        >= 2
}

pub fn interpret(
    rows: &[HistoryRow],
    range: HistoryRange,
    axis_start_ms: i64,
    axis_end_ms: i64,
) -> HistoryReady {
    HistoryReady {
        range,
        axis_start_ms,
        axis_end_ms,
        state: ready_state(rows, range),
    }
}

fn ready_state(rows: &[HistoryRow], range: HistoryRange) -> HistoryReadyState {
    if rows.is_empty() {
        return HistoryReadyState::Empty;
    }
    let success = rows
        .iter()
        .filter(|row| row.status == HistoryStatus::Success)
        .count();
    if success == 0 {
        return HistoryReadyState::OnlyFailures {
            count: rows.len() as u64,
        };
    }
    let series = build_series(rows, range);
    if series.is_empty() {
        HistoryReadyState::NoNumeric
    } else {
        HistoryReadyState::Charts(series)
    }
}

fn build_series(rows: &[HistoryRow], range: HistoryRange) -> Vec<HistorySeries> {
    let mut keys = Vec::new();
    for row in rows {
        if row.status != HistoryStatus::Success {
            continue;
        }
        for point in &row.points {
            if drawable(point).is_none() {
                continue;
            }
            if !keys.iter().any(|key| key == &point.quota_key) {
                keys.push(point.quota_key.clone());
            }
        }
    }
    keys.into_iter()
        .filter_map(|key| series_for_key(rows, range, &key))
        .collect()
}

fn series_for_key(
    rows: &[HistoryRow],
    range: HistoryRange,
    quota_key: &str,
) -> Option<HistorySeries> {
    let width = range.bucket_width_ms();
    let mut samples = Vec::new();
    let mut label_spec_json = String::new();
    for row in rows {
        if row.status != HistoryStatus::Success {
            continue;
        }
        if let Some(point) = row
            .points
            .iter()
            .rev()
            .find(|point| point.quota_key == quota_key)
        {
            if let Some(drawable) = drawable(point) {
                label_spec_json = point.label_spec_json.clone();
                samples.push((row.captured_at_ms, drawable));
            }
        }
    }
    if samples.is_empty() {
        return None;
    }
    samples.sort_by_key(|(captured, _)| *captured);
    let mut buckets: Vec<(i64, Drawable)> = Vec::new();
    for (captured, drawable) in samples {
        let start = floor_bucket(captured, width);
        if let Some(last) = buckets.last_mut() {
            if last.0 == start {
                *last = (start, drawable);
                continue;
            }
        }
        buckets.push((start, drawable));
    }

    let mut segments = Vec::new();
    let mut current: Option<HistorySegment> = None;
    let mut previous_bucket: Option<i64> = None;
    for (bucket, drawable) in buckets {
        let gap = previous_bucket.is_some_and(|previous| bucket - previous > width);
        let same = current.as_ref().is_some_and(|segment| {
            segment.unit == drawable.unit && segment.y_kind == drawable.y_kind
        });
        if !same {
            if let Some(segment) = current.take() {
                segments.push(segment);
            }
            current = Some(HistorySegment {
                unit: drawable.unit,
                y_kind: drawable.y_kind,
                points: Vec::new(),
            });
        }
        current.as_mut().unwrap().points.push(SeriesPoint {
            bucket_start_ms: bucket,
            y: drawable.y,
            limit: drawable.limit,
            gap_before: gap || !same,
        });
        previous_bucket = Some(bucket);
    }
    if let Some(segment) = current {
        segments.push(segment);
    }
    // 每个序列的第一点不是断档。
    if let Some(first) = segments
        .first_mut()
        .and_then(|segment| segment.points.first_mut())
    {
        first.gap_before = false;
    }
    Some(HistorySeries {
        quota_key: quota_key.to_string(),
        label_spec_json,
        segments,
    })
}

struct Drawable {
    unit: HistoryUnit,
    y_kind: HistoryYKind,
    y: f64,
    limit: Option<f64>,
}

fn drawable(point: &HistoryPointRow) -> Option<Drawable> {
    match point.value_kind {
        HistoryValueKind::Metered => {
            let y = point.used?;
            let unit = point.unit?;
            Some(Drawable {
                unit,
                y_kind: HistoryYKind::MeteredUsed,
                y,
                limit: point.limit_value,
            })
        }
        HistoryValueKind::Balance => {
            let y = point.remaining?;
            let unit = point.unit?;
            Some(Drawable {
                unit,
                y_kind: HistoryYKind::BalanceRemaining,
                y,
                limit: None,
            })
        }
        HistoryValueKind::NonNumeric => None,
    }
}

fn floor_bucket(captured_at_ms: i64, width: i64) -> i64 {
    captured_at_ms.div_euclid(width) * width
}

/// 数据跨度两侧各留出的比例。折线不贴边，小波动也不会被整段刻度压扁。
const Y_PAD_RATIO: f64 = 0.15;
/// 样本时间跨度两侧各留出的比例。横轴按样本展开，不按空白窗口。
const X_PAD_RATIO: f64 = 1.0 / 16.0;
/// 所有点落在同一时刻时，横轴向两侧各扩这么多。
const X_FLAT_PAD_MS: i64 = 15 * 60 * 1000;

/// 整条序列能共用一把纵轴时的刻度。单位或已用/剩余含义不一致时返回 `None`。
///
/// 刻度跟着样本走，两侧留白。百分比不再钉死 0–100：用量接近 0 时，
/// 整段刻度会把折线压成一条贴底的线，数字对不上这条线。
pub struct SharedYScale {
    pub min: f64,
    pub max: f64,
    pub unit: HistoryUnit,
    pub y_kind: HistoryYKind,
}

/// 画出来的横轴。选中窗口只决定有哪些点；点都挤在窗口一端时，按窗口画会缩成一个点。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeriesXBounds {
    pub start_ms: i64,
    pub end_ms: i64,
}

pub fn shared_y_scale(segments: &[HistorySegment]) -> Option<SharedYScale> {
    let first = segments.first()?;
    if segments
        .iter()
        .any(|segment| segment.unit != first.unit || segment.y_kind != first.y_kind)
    {
        return None;
    }
    let (min, max) = y_extent(segments.iter().flat_map(|segment| &segment.points))?;
    let (min, max) = padded_y_domain(min, max, first.unit);
    Some(SharedYScale {
        min,
        max,
        unit: first.unit,
        y_kind: first.y_kind,
    })
}

pub fn series_x_bounds(segments: &[HistorySegment]) -> Option<SeriesXBounds> {
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
    if !any {
        return None;
    }
    let span = max.saturating_sub(min);
    if span <= 0 {
        return Some(SeriesXBounds {
            start_ms: min.saturating_sub(X_FLAT_PAD_MS),
            end_ms: max.saturating_add(X_FLAT_PAD_MS),
        });
    }
    let pad = ((span as f64) * X_PAD_RATIO).round().max(1.0) as i64;
    Some(SeriesXBounds {
        start_ms: min.saturating_sub(pad),
        end_ms: max.saturating_add(pad),
    })
}

pub fn plot_segment(
    segment: &HistorySegment,
    axis_start_ms: i64,
    axis_end_ms: i64,
) -> (Option<&'static str>, Vec<PlottedPoint>) {
    let (y_min, y_max) = y_domain(segment);
    let suffix = (segment.unit == HistoryUnit::Percentage).then_some("%");
    (
        suffix,
        plot_on_scale(segment, axis_start_ms, axis_end_ms, y_min, y_max),
    )
}

pub fn plot_on_scale(
    segment: &HistorySegment,
    axis_start_ms: i64,
    axis_end_ms: i64,
    y_min: f64,
    y_max: f64,
) -> Vec<PlottedPoint> {
    let span = (axis_end_ms - axis_start_ms).max(1) as f64;
    let y_span = (y_max - y_min).abs();
    segment
        .points
        .iter()
        .map(|point| {
            let x_ratio = ((point.bucket_start_ms - axis_start_ms) as f64 / span).clamp(0.0, 1.0);
            let y_ratio = if y_span < f64::EPSILON {
                0.5
            } else {
                ((point.y - y_min) / y_span).clamp(0.0, 1.0)
            };
            PlottedPoint {
                x_ratio,
                y_ratio,
                gap_before: point.gap_before,
            }
        })
        .collect()
}

fn y_domain(segment: &HistorySegment) -> (f64, f64) {
    y_extent(segment.points.iter())
        .map(|(min, max)| padded_y_domain(min, max, segment.unit))
        .unwrap_or((0.0, 0.0))
}

fn y_extent<'a>(points: impl Iterator<Item = &'a SeriesPoint>) -> Option<(f64, f64)> {
    let mut min = f64::MAX;
    let mut max = f64::MIN;
    let mut any = false;
    for point in points {
        any = true;
        min = min.min(point.y);
        max = max.max(point.y);
    }
    any.then_some((min, max))
}

/// 样本落在 0..=100 里时，留白不探出这段；样本本身超出时不裁掉。
fn padded_y_domain(min: f64, max: f64, unit: HistoryUnit) -> (f64, f64) {
    let span = (max - min).abs();
    if span < f64::EPSILON {
        return (min, max);
    }
    let pad = span * Y_PAD_RATIO;
    let mut lo = min - pad;
    let mut hi = max + pad;
    if unit == HistoryUnit::Percentage {
        if min >= 0.0 {
            lo = lo.max(0.0);
        }
        if max <= 100.0 {
            hi = hi.min(100.0);
        }
    }
    (lo, hi)
}

#[cfg(test)]
mod tests {
    use super::super::sample::{HistoryUnit, HistoryValueKind};
    use super::*;
    use crate::history::{HistoryPointRow, HistoryRow, HistoryStatus};

    fn row(
        id: i64,
        captured: i64,
        status: HistoryStatus,
        points: Vec<HistoryPointRow>,
    ) -> HistoryRow {
        HistoryRow {
            sample_id: id,
            captured_at_ms: captured,
            provider_id: "codex".to_string(),
            status,
            error_kind: None,
            failure_reason: None,
            failure_reason_payload: None,
            failure_advice_json: None,
            detail: None,
            refresh_reason: None,
            points,
        }
    }

    fn metered(key: &str, used: f64, unit: HistoryUnit) -> HistoryPointRow {
        HistoryPointRow {
            quota_key: key.to_string(),
            quota_type: "general".to_string(),
            label_spec_json: "{\"Session\":null}".to_string(),
            value_kind: HistoryValueKind::Metered,
            unit: Some(unit),
            used: Some(used),
            remaining: None,
            limit_value: Some(100.0),
            reset_at_secs: None,
        }
    }

    #[test]
    fn only_failures_are_not_an_empty_chart() {
        let ready = interpret(
            &[row(1, 1, HistoryStatus::Failed, Vec::new())],
            HistoryRange::Last24Hours,
            0,
            1,
        );
        assert_eq!(ready.state, HistoryReadyState::OnlyFailures { count: 1 });
    }

    #[test]
    fn success_without_numeric_points_is_not_empty() {
        let ready = interpret(
            &[row(1, 1, HistoryStatus::Success, Vec::new())],
            HistoryRange::Last24Hours,
            0,
            1,
        );
        assert_eq!(ready.state, HistoryReadyState::NoNumeric);
    }

    #[test]
    fn bucket_keeps_the_last_point_and_opens_a_gap() {
        let width = HistoryRange::Last24Hours.bucket_width_ms();
        let ready = interpret(
            &[
                row(
                    1,
                    width,
                    HistoryStatus::Success,
                    vec![metered("session", 10.0, HistoryUnit::Percentage)],
                ),
                row(
                    2,
                    width + 1,
                    HistoryStatus::Success,
                    vec![metered("session", 40.0, HistoryUnit::Percentage)],
                ),
                row(
                    3,
                    width * 3,
                    HistoryStatus::Success,
                    vec![metered("session", 80.0, HistoryUnit::Percentage)],
                ),
            ],
            HistoryRange::Last24Hours,
            0,
            width * 4,
        );
        let HistoryReadyState::Charts(series) = ready.state else {
            panic!("expected charts");
        };
        assert_eq!(series[0].segments[0].points.len(), 2);
        assert_eq!(series[0].segments[0].points[0].y, 40.0);
        assert_eq!(series[0].segments[0].points[0].limit, Some(100.0));
        assert!(series[0].segments[0].points[1].gap_before);
    }

    #[test]
    fn unit_change_starts_a_new_segment() {
        let ready = interpret(
            &[
                row(
                    1,
                    0,
                    HistoryStatus::Success,
                    vec![metered("weekly", 10.0, HistoryUnit::Percentage)],
                ),
                row(
                    2,
                    HistoryRange::Last24Hours.bucket_width_ms(),
                    HistoryStatus::Success,
                    vec![metered("weekly", 200.0, HistoryUnit::Amount)],
                ),
            ],
            HistoryRange::Last24Hours,
            0,
            3_600_000,
        );
        let HistoryReadyState::Charts(series) = ready.state else {
            panic!("expected charts");
        };
        assert_eq!(series[0].segments.len(), 2);
        let (_, points) = plot_segment(&series[0].segments[1], 0, 3_600_000);
        assert_eq!(points[0].y_ratio, 0.5);
    }

    fn segment(unit: HistoryUnit, points: Vec<(i64, f64)>) -> HistorySegment {
        HistorySegment {
            unit,
            y_kind: HistoryYKind::MeteredUsed,
            points: points
                .into_iter()
                .map(|(bucket_start_ms, y)| SeriesPoint {
                    bucket_start_ms,
                    y,
                    limit: None,
                    gap_before: false,
                })
                .collect(),
        }
    }

    #[test]
    fn percentage_scale_follows_samples_instead_of_zero_to_hundred() {
        let low = segment(HistoryUnit::Percentage, vec![(1_000, 0.0), (61_000, 2.0)]);
        let scale = shared_y_scale(std::slice::from_ref(&low)).expect("scale");
        let expected_max = 2.0 + 2.0 * 0.15;
        assert_eq!(scale.min, 0.0);
        assert!((scale.max - expected_max).abs() < 1e-12);
        assert!(scale.max < 10.0);

        let full = segment(HistoryUnit::Percentage, vec![(1_000, 0.0), (61_000, 100.0)]);
        let scale = shared_y_scale(std::slice::from_ref(&full)).expect("scale");
        assert_eq!((scale.min, scale.max), (0.0, 100.0));

        let flat = segment(HistoryUnit::Percentage, vec![(1_000, 0.0), (61_000, 0.0)]);
        let scale = shared_y_scale(std::slice::from_ref(&flat)).expect("scale");
        assert_eq!((scale.min, scale.max), (0.0, 0.0));
    }

    #[test]
    fn x_bounds_follow_samples_not_the_empty_window() {
        let samples = segment(
            HistoryUnit::Percentage,
            vec![(86_400_000 - 120_000, 1.0), (86_400_000 - 60_000, 2.0)],
        );
        let bounds = series_x_bounds(std::slice::from_ref(&samples)).expect("bounds");
        let plotted = plot_on_scale(&samples, bounds.start_ms, bounds.end_ms, 0.0, 2.3);
        assert!(plotted[0].x_ratio < 0.2);
        assert!(plotted[1].x_ratio > 0.8);
        assert!(bounds.end_ms - bounds.start_ms < 60 * 60 * 1000);
    }
}
