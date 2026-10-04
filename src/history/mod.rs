//! 用量历史：把一次已采纳的刷新变成可查询的本地事实。
//!
//! 记录、存储、聚合分开。SQLite 适配器只执行 effect 里已经算好的 cutoff，不读设置。

mod job;
mod redact;
mod retention;
mod sample;
mod series;
mod sqlite;
mod store;

pub use job::{HistoryJob, HistoryLoadRequest, ProviderRetentionCutoff};
pub use redact::scrub_history_detail;
pub use retention::{cutoff_ms, query_window, retention_cutoffs, DAY_MS};
pub use sample::{capture_sample, HistoryStatus, QuotaHistoryPoint, QuotaHistorySample};
pub use series::{
    interpret, plot_segment, HistoryChart, HistoryLoadOutcome, HistoryRange, HistoryReady,
    HistoryReadyState, HistorySegment, HistorySeries, PlottedPoint,
};
pub use sqlite::SqliteQuotaHistoryStore;
pub use store::{
    HistoryError, HistoryPointRow, HistoryRangeQuery, HistoryRow, QuotaHistoryReader,
    QuotaHistoryStore, QuotaHistoryWriter,
};

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}
