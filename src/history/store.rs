use super::sample::{HistoryStatus, HistoryUnit, HistoryValueKind, QuotaHistorySample};

#[derive(Debug)]
pub struct HistoryError {
    message: String,
}

impl HistoryError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for HistoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for HistoryError {}

#[derive(Debug, Clone)]
pub struct HistoryRangeQuery {
    pub provider_id: String,
    pub captured_from_ms: i64,
    pub captured_to_ms: i64,
    pub include_non_success: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HistoryPointRow {
    pub quota_key: String,
    pub quota_type: String,
    pub label_spec_json: String,
    pub value_kind: HistoryValueKind,
    pub unit: Option<HistoryUnit>,
    pub used: Option<f64>,
    pub remaining: Option<f64>,
    pub limit_value: Option<f64>,
    pub reset_at_secs: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HistoryRow {
    pub sample_id: i64,
    pub captured_at_ms: i64,
    pub provider_id: String,
    pub status: HistoryStatus,
    pub error_kind: Option<String>,
    pub failure_reason: Option<String>,
    pub failure_reason_payload: Option<String>,
    pub failure_advice_json: Option<String>,
    pub detail: Option<String>,
    pub refresh_reason: Option<String>,
    pub points: Vec<HistoryPointRow>,
}

pub trait QuotaHistoryWriter: Send {
    fn append(&mut self, sample: &QuotaHistorySample) -> Result<(), HistoryError>;
    fn purge_provider(&mut self, provider_id: &str) -> Result<u64, HistoryError>;
    fn purge_provider_before(
        &mut self,
        provider_id: &str,
        captured_before_ms: i64,
    ) -> Result<u64, HistoryError>;
    fn purge_all(&mut self) -> Result<u64, HistoryError>;
}

pub trait QuotaHistoryReader: Send {
    fn load_rows(&mut self, query: &HistoryRangeQuery) -> Result<Vec<HistoryRow>, HistoryError>;
}

/// 读写合一的历史存储契约，供历史线程以抽象方式持有。`sqlite` 是唯一实现。
pub trait QuotaHistoryStore: QuotaHistoryReader + QuotaHistoryWriter {}

impl<T> QuotaHistoryStore for T where T: QuotaHistoryReader + QuotaHistoryWriter {}
