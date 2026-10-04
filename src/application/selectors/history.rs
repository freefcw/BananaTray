//! 折线视图构建：`HistoryReady` → 归一化坐标的 `HistoryChartView`。
//! 设置页与托盘弹窗共用同一套聚合结果，只在外层布局上不同。

use super::format::format_quota_label_spec;
use super::{HistoryChartView, HistoryPointView, HistorySegmentView};
use crate::history::{plot_segment, HistoryReady, HistoryReadyState, HistorySeries};
use crate::models::QuotaLabelSpec;

/// 非 Charts 状态（空、全失败、无数值）统一返回空列表，由 UI 决定要不要占位。
/// 少于 2 个点的序列画不出折线，只剩角落一个点，直接过滤。
pub fn history_charts_view(ready: &HistoryReady) -> Vec<HistoryChartView> {
    let HistoryReadyState::Charts(series) = &ready.state else {
        return Vec::new();
    };
    series
        .iter()
        .filter(|item| drawable_points(item) >= 2)
        .map(|item| history_chart_view(item, ready.axis_start_ms, ready.axis_end_ms))
        .collect()
}

fn drawable_points(series: &HistorySeries) -> usize {
    series
        .segments
        .iter()
        .map(|segment| segment.points.len())
        .sum()
}

fn history_chart_view(
    series: &HistorySeries,
    axis_start_ms: i64,
    axis_end_ms: i64,
) -> HistoryChartView {
    let mut title = history_series_title(&series.label_spec_json);
    let mut suffix = None;
    let mut mixed_suffix = false;
    let segments = series
        .segments
        .iter()
        .map(|segment| {
            let (next_suffix, points) = plot_segment(segment, axis_start_ms, axis_end_ms);
            match (suffix, next_suffix) {
                (None, Some(next)) => suffix = Some(next),
                (Some(current), Some(next)) if current != next => mixed_suffix = true,
                _ => {}
            }
            HistorySegmentView {
                points: points
                    .into_iter()
                    .map(|point| HistoryPointView {
                        x_ratio: point.x_ratio,
                        y_ratio: point.y_ratio,
                        gap_before: point.gap_before,
                    })
                    .collect(),
            }
        })
        .collect();
    if let Some(suffix) = suffix.filter(|_| !mixed_suffix) {
        title.push(' ');
        title.push_str(suffix);
    }
    HistoryChartView { title, segments }
}

fn history_series_title(label_spec_json: &str) -> String {
    let label_spec = serde_json::from_str::<QuotaLabelSpec>(label_spec_json)
        .unwrap_or_else(|_| QuotaLabelSpec::Raw(label_spec_json.to_string()));
    format_quota_label_spec(&label_spec)
}

#[cfg(test)]
mod tests {
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
        let charts = history_charts_view(&ready(vec![series("a", 1), series("b", 3)]));
        assert_eq!(charts.len(), 1);

        let charts = history_charts_view(&ready(vec![series("a", 1)]));
        assert!(charts.is_empty());
    }
}
