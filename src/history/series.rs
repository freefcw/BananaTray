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
    pub y: f64,
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
            })
        }
        HistoryValueKind::Balance => {
            let y = point.remaining?;
            let unit = point.unit?;
            Some(Drawable {
                unit,
                y_kind: HistoryYKind::BalanceRemaining,
                y,
            })
        }
        HistoryValueKind::NonNumeric => None,
    }
}

fn floor_bucket(captured_at_ms: i64, width: i64) -> i64 {
    captured_at_ms.div_euclid(width) * width
}

pub fn plot_segment(
    segment: &HistorySegment,
    axis_start_ms: i64,
    axis_end_ms: i64,
) -> (Option<&'static str>, Vec<PlottedPoint>) {
    let span = (axis_end_ms - axis_start_ms).max(1) as f64;
    let (y_min, y_max) = y_domain(segment);
    let y_span = (y_max - y_min).abs();
    let points = segment
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
        .collect();
    let suffix = (segment.unit == HistoryUnit::Percentage).then_some("%");
    (suffix, points)
}

fn y_domain(segment: &HistorySegment) -> (f64, f64) {
    let mut min = f64::MAX;
    let mut max = f64::MIN;
    for point in &segment.points {
        min = min.min(point.y);
        max = max.max(point.y);
    }
    if segment.unit == HistoryUnit::Percentage {
        return (0.0, max.max(100.0));
    }
    if (max - min).abs() < f64::EPSILON {
        return (min, min);
    }
    (min, max)
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
}
