use serde::{Deserialize, Serialize};
use std::time::Instant;

use crate::models::provider::{
    ProviderCapability, ProviderId, ProviderKind, ProviderMetadata, SettingsCapability,
};

use super::{ProviderFailure, QuotaInfo, RefreshData, StatusLevel};

/// 元数据代理方法生成宏：保持 `provider.display_name()` 等 API 不变，
/// 消除手写代理的样板代码。新增 ProviderMetadata 字段时只需加一行。
macro_rules! delegate_metadata {
    ($($method:ident -> $field:ident),* $(,)?) => {
        $(pub fn $method(&self) -> &str { &self.metadata.$field })*
    };
}

/// 错误类型分类（用于 UI 决定操作）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ErrorKind {
    #[default]
    Unknown,
    /// 配置缺失 → 显示"打开配置"
    ConfigMissing,
    /// 认证问题 → 显示"打开配置"
    AuthRequired,
    /// 网络问题 → 显示"重试"
    NetworkError,
}

/// 连接状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionStatus {
    Connected,
    Disconnected,
    Refreshing,
    Error,
}

/// 上次刷新的结果状态（结构化，不含展示文案）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpdateStatus {
    /// 最近一次刷新失败
    Failed,
}

/// 单个 Provider 的完整运行时状态
///
/// ## 状态转换规则
///
/// ```text
/// ┌──────────────┐
/// │ Disconnected │──mark_refreshing()──→ Refreshing
/// └──────────────┘                          │
///       ↑                              ┌────┴────┐
///  mark_unavailable()            succeeded()   failed()
///  (非 Connected 时)                 │      ┌───┴───┐
///                              Connected  有旧数据？ 无旧数据？
///                                         Connected  Error
/// ```
///
/// - `mark_refresh_failed`: 有旧配额数据 → 保持 Connected（展示陈旧数据）；
///   无旧数据 → Error（触发 UI 空状态/错误提示）
/// - `mark_unavailable`: 仅在非 Connected 时回退到 Disconnected
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderStatus {
    /// 统一标识符（内置 = BuiltIn(kind)，自定义 = Custom(id)）
    #[serde(default = "default_provider_id")]
    pub provider_id: ProviderId,
    /// 静态元数据（名称、图标、链接等）
    pub metadata: ProviderMetadata,
    pub connection: ConnectionStatus,
    pub quotas: Vec<QuotaInfo>,
    /// 账号邮箱（可选，用于 UI 展示）
    pub account_email: Option<String>,
    /// 账号层级（如 "Pro", "Max", "Free", "Business"）
    pub account_tier: Option<String>,
    /// 最近一次成功刷新实际使用的数据源；若为空，则回退到静态 metadata.source_label
    #[serde(default)]
    pub runtime_source_label: Option<String>,
    /// 上次刷新的结果状态（结构化，selector 层负责 i18n 格式化）
    #[serde(default)]
    pub update_status: Option<UpdateStatus>,
    /// 最近一次失败的稳定语义载荷
    #[serde(default)]
    pub last_failure: Option<ProviderFailure>,
    /// 错误类型分类（用于 UI 决定操作）
    #[serde(default)]
    pub error_kind: ErrorKind,
    /// 上次成功刷新的时刻（不序列化，用于计算相对时间）
    #[serde(skip)]
    pub last_refreshed_instant: Option<Instant>,
    /// 设置 UI 能力声明（运行时由 ProviderManager 填充，不序列化）
    #[serde(skip)]
    pub settings_capability: SettingsCapability,
    /// Provider 能力层级（运行时由 ProviderManager 填充，不序列化）
    #[serde(skip)]
    pub provider_capability: ProviderCapability,
}

/// serde 默认值：反序列化旧数据时，provider_id 用 Claude 作占位
fn default_provider_id() -> ProviderId {
    ProviderId::BuiltIn(ProviderKind::Claude)
}

impl ProviderStatus {
    /// 获取 ProviderKind 分类（从 provider_id 派生）
    pub fn kind(&self) -> ProviderKind {
        self.provider_id.kind()
    }

    /// 创建运行时 Provider 状态。
    ///
    /// 调用方必须保证 `provider_id.kind()` 与 `metadata.kind` 一致。
    /// 这里使用 `debug_assert_eq!` 在开发/测试阶段尽早暴露错误组合，
    /// release 构建则保持零额外开销。
    pub fn new(provider_id: ProviderId, metadata: ProviderMetadata) -> Self {
        debug_assert_eq!(
            provider_id.kind(),
            metadata.kind,
            "provider_id 与 metadata.kind 不一致: {:?} vs {:?}",
            provider_id,
            metadata.kind
        );
        Self {
            provider_id,
            metadata,
            connection: ConnectionStatus::Disconnected,
            quotas: vec![],
            account_email: None,
            account_tier: None,
            runtime_source_label: None,
            update_status: None,
            last_failure: None,
            error_kind: ErrorKind::default(),
            last_refreshed_instant: None,
            settings_capability: SettingsCapability::default(),
            provider_capability: ProviderCapability::default(),
        }
    }

    pub fn mark_refreshing(&mut self) {
        self.connection = ConnectionStatus::Refreshing;
    }

    pub fn mark_refresh_succeeded(&mut self, data: RefreshData) {
        self.quotas = data.quotas;
        self.account_email = data.account_email;
        self.account_tier = data.account_tier;
        self.runtime_source_label = data.source_label;
        self.connection = ConnectionStatus::Connected;
        self.last_refreshed_instant = Some(Instant::now());
        self.update_status = None;
        self.last_failure = None;
        self.error_kind = ErrorKind::default();
    }

    pub fn mark_unavailable(&mut self, failure: ProviderFailure) {
        if self.connection != ConnectionStatus::Connected {
            self.connection = ConnectionStatus::Disconnected;
        }
        self.last_failure = Some(failure);
    }

    /// 同步 provider 定义层数据（metadata + settings capability），保留运行时状态。
    ///
    /// 返回 true 表示 definition 发生变化。
    pub fn sync_definition_from(&mut self, other: &ProviderStatus) -> bool {
        let mut changed = false;
        if self.metadata != other.metadata {
            self.metadata = other.metadata.clone();
            changed = true;
        }
        if self.settings_capability != other.settings_capability {
            self.settings_capability = other.settings_capability.clone();
            changed = true;
        }
        if self.provider_capability != other.provider_capability {
            self.provider_capability = other.provider_capability;
            changed = true;
        }
        changed
    }

    /// 标记刷新失败，同时设置错误类型
    pub fn mark_refresh_failed(&mut self, failure: ProviderFailure, error_kind: ErrorKind) {
        if self.quotas.is_empty() {
            self.connection = ConnectionStatus::Error;
        } else {
            self.connection = ConnectionStatus::Connected;
        }
        self.update_status = Some(UpdateStatus::Failed);
        self.last_failure = Some(failure);
        self.error_kind = error_kind;
    }

    // 元数据代理方法（由宏生成，保持 30+ 处调用点兼容）
    delegate_metadata!(
        display_name -> display_name,
        icon_asset -> icon_asset,
        dashboard_url -> dashboard_url,
        brand_name -> brand_name,
        account_hint -> account_hint,
    );

    pub fn source_label(&self) -> &str {
        self.runtime_source_label
            .as_deref()
            .unwrap_or(&self.metadata.source_label)
    }

    pub fn supports_refresh(&self) -> bool {
        self.provider_capability.supports_refresh()
    }

    /// 获取最高用量的状态等级（用于总览显示）
    pub fn worst_status(&self) -> StatusLevel {
        self.quotas
            .iter()
            .map(|q| q.status_level())
            .max()
            .unwrap_or(StatusLevel::Green)
    }
}

// ============================================================================
// Tests
// ============================================================================
