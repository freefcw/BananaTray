use super::sample::QuotaHistorySample;
use super::series::HistoryRange;

/// 历史线程上的一个任务。cutoff 由前台算好，线程不读设置。
#[derive(Debug)]
pub enum HistoryJob {
    Append {
        sample: QuotaHistorySample,
        cutoff_ms: i64,
    },
    Load(HistoryLoadRequest),
    PurgeProvider {
        provider_id: String,
    },
    PurgeAll,
    ApplyRetention {
        targets: Vec<ProviderRetentionCutoff>,
    },
}

#[derive(Debug, Clone)]
pub struct HistoryLoadRequest {
    pub request_id: u64,
    pub provider_id: String,
    /// 选中的查询窗口。画出来的横轴是窗口内样本的起止时间。
    pub axis_start_ms: i64,
    pub axis_end_ms: i64,
    /// 实际查询起点。不早于保留期限。
    pub captured_from_ms: i64,
    pub captured_to_ms: i64,
    pub range: HistoryRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderRetentionCutoff {
    pub provider_id: String,
    pub cutoff_ms: i64,
}
