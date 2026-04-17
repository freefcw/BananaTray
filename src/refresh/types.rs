use crate::models::{ErrorKind, ProviderFailure, ProviderId, ProviderStatus, RefreshData};

/// 刷新触发原因
#[derive(Debug, Clone, Copy)]
pub enum RefreshReason {
    Startup,
    Periodic,
    Manual,
    ProviderToggled,
}

/// 发送给协调器的请求
#[derive(Debug)]
pub enum RefreshRequest {
    RefreshAll {
        reason: RefreshReason,
    },
    RefreshOne {
        id: ProviderId,
        reason: RefreshReason,
    },
    UpdateConfig {
        interval_mins: u64,
        enabled: Vec<ProviderId>,
    },
    /// 热重载自定义 Provider（重建 ProviderManager 快照）
    ReloadProviders,
    /// 预留给未来显式关闭协调器的退出路径；当前由 channel 关闭兜底。
    #[allow(dead_code)]
    Shutdown,
}

/// 协调器发出的事件
#[derive(Debug)]
pub enum RefreshEvent {
    Started {
        id: ProviderId,
    },
    Finished(RefreshOutcome),
    /// 自定义 Provider 热重载完成，携带最新的 Provider 状态列表
    ProvidersReloaded {
        statuses: Vec<ProviderStatus>,
    },
}

/// 单个 Provider 的刷新结果
#[derive(Debug)]
pub struct RefreshOutcome {
    pub id: ProviderId,
    pub result: RefreshResult,
}

/// 刷新结果类型
#[derive(Debug)]
pub enum RefreshResult {
    Success {
        data: RefreshData,
    },
    Unavailable {
        failure: ProviderFailure,
    },
    Failed {
        failure: ProviderFailure,
        error_kind: ErrorKind,
    },
    SkippedCooldown,
    SkippedInFlight,
    SkippedDisabled,
}
