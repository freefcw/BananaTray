//! Claude Provider 探针抽象
//!
//! 定义了获取配额的统一接口和选择模式。

use crate::models::QuotaInfo;
use anyhow::Result;

/// 获取方式选择模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(dead_code)]
pub enum ProbeMode {
    /// 自动选择：优先 API，失败回退 CLI
    #[default]
    Auto,
    /// 强制使用 CLI
    Cli,
    /// 强制使用 API
    Api,
}

/// Probe Trait：数据源选择的抽象。
///
/// 这里描述的是“去哪里拿数据”（CLI / API），而不是“如何解析同一份数据”。
/// 因此它不应与 Antigravity 的 `ParseStrategy` 合并成同一个通用 trait。
pub trait UsageProbe: Send + Sync {
    /// 执行配额获取
    fn probe(&self) -> Result<Vec<QuotaInfo>>;

    /// 检查该获取方式是否可用
    fn is_available(&self) -> bool;
}
