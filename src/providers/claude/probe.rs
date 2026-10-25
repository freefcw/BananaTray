//! Claude Provider 探针抽象
//!
//! 定义了获取配额的统一接口。

use crate::models::QuotaInfo;
use crate::providers::ProviderResult;

/// Probe Trait：数据源选择的抽象。
///
/// 这里描述的是“去哪里拿数据”（CLI / API），而不是“如何解析同一份数据”。
/// 因此它不应与 Antigravity 的 `ParseStrategy` 合并成同一个通用 trait。
pub trait UsageProbe: Send + Sync {
    /// 执行配额获取
    fn probe(&self) -> ProviderResult<Vec<QuotaInfo>>;

    /// 检查该获取方式是否可用
    fn is_available(&self) -> bool;
}
