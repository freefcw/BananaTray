use super::provider::{ProviderKind, ProviderMetadata};
use serde::{Deserialize, Serialize};
use std::time::Instant;

// ============================================================================
// 配额类型
// ============================================================================

/// 配额类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuotaType {
    /// 5h 滑动窗口会话配额
    Session,
    /// 周配额（所有模型合计）
    Weekly,
    /// 按模型的周配额（如 Opus / Sonnet）
    ModelSpecific(String),
    /// 基于金额的信用额度
    Credit,
    /// 通用/不确定类型
    General,
}

// ============================================================================
// 用量状态等级
// ============================================================================

/// 用量状态等级（用于颜色编码）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusLevel {
    Green,
    Yellow,
    Red,
}

impl StatusLevel {
    /// 数值严重程度，用于排序比较
    fn severity(self) -> u8 {
        match self {
            Self::Green => 0,
            Self::Yellow => 1,
            Self::Red => 2,
        }
    }
}

impl PartialOrd for StatusLevel {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for StatusLevel {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.severity().cmp(&other.severity())
    }
}

// ============================================================================
// 用量信息
// ============================================================================

/// 用量配额信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaInfo {
    /// 已使用量
    pub used: f64,
    /// 总配额
    pub limit: f64,
    /// 配额类型标签（如 "Session (5h)", "Weekly", "Pro"）
    pub label: String,
    /// 配额类型
    #[serde(default = "default_quota_type")]
    pub quota_type: QuotaType,
    /// 重置时间描述（如 "Resets in 2h 15m"）
    pub reset_at: Option<String>,
}

fn default_quota_type() -> QuotaType {
    QuotaType::General
}

impl QuotaInfo {
    pub fn new(label: impl Into<String>, used: f64, limit: f64) -> Self {
        Self {
            used,
            limit,
            label: label.into(),
            quota_type: QuotaType::General,
            reset_at: None,
        }
    }

    /// 创建带完整信息的配额
    pub fn with_details(
        label: impl Into<String>,
        used: f64,
        limit: f64,
        quota_type: QuotaType,
        reset_at: Option<String>,
    ) -> Self {
        Self {
            used,
            limit,
            label: label.into(),
            quota_type,
            reset_at,
        }
    }

    /// 使用百分比 (0.0 - 100.0)
    pub fn percentage(&self) -> f64 {
        if self.limit <= 0.0 {
            return 0.0;
        }
        (self.used / self.limit * 100.0).min(100.0)
    }

    /// 是否是纯百分比模式（limit == 100.0，数据本身就是百分比）
    #[allow(dead_code)]
    pub fn is_percentage_mode(&self) -> bool {
        (self.limit - 100.0).abs() < f64::EPSILON
    }

    /// 状态等级：Green / Yellow / Red (基于剩余量)
    pub fn status_level(&self) -> StatusLevel {
        let pct = self.percentage();
        let remaining_pct = (100.0 - pct).max(0.0);

        if remaining_pct > 50.0 {
            StatusLevel::Green
        } else if remaining_pct >= 20.0 {
            StatusLevel::Yellow
        } else {
            StatusLevel::Red
        }
    }
}

// ============================================================================
// 连接状态 & Provider 状态
// ============================================================================

/// 连接状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionStatus {
    Connected,
    Disconnected,
    Refreshing,
    Error,
}

/// 单个 Provider 的完整状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderStatus {
    pub kind: ProviderKind,
    /// 静态元数据（名称、图标、链接等）
    pub metadata: ProviderMetadata,
    /// 启用状态（从设置读取）
    pub enabled: bool,
    pub connection: ConnectionStatus,
    pub quotas: Vec<QuotaInfo>,
    /// 账号邮箱（可选，用于 UI 展示）
    pub account_email: Option<String>,
    /// 是否为付费版
    pub is_paid: bool,
    /// 账号层级（如 "Pro", "Max", "Free", "Business"）
    pub account_tier: Option<String>,
    /// 上次更新时间描述（仅用于错误/断连状态的静态文案）
    pub last_updated_at: Option<String>,
    /// 最近一次刷新失败时的提示文案
    pub error_message: Option<String>,
    /// 上次成功刷新的时刻（不序列化，用于计算相对时间）
    #[serde(skip)]
    pub last_refreshed_instant: Option<Instant>,
}

impl ProviderStatus {
    pub fn new(metadata: ProviderMetadata) -> Self {
        Self {
            kind: metadata.kind,
            metadata,
            enabled: true,
            connection: ConnectionStatus::Disconnected,
            quotas: vec![],
            account_email: None,
            is_paid: false,
            account_tier: None,
            last_updated_at: None,
            error_message: None,
            last_refreshed_instant: None,
        }
    }

    pub fn mark_refreshing(&mut self) {
        self.connection = ConnectionStatus::Refreshing;
    }

    pub fn mark_refresh_succeeded(&mut self, quotas: Vec<QuotaInfo>) {
        self.quotas = quotas;
        self.connection = ConnectionStatus::Connected;
        self.last_refreshed_instant = Some(Instant::now());
        self.last_updated_at = None;
        self.error_message = None;
    }

    pub fn mark_unavailable(&mut self, message: String) {
        if self.connection != ConnectionStatus::Connected {
            self.connection = ConnectionStatus::Disconnected;
        }
        self.error_message = Some(message);
    }

    pub fn mark_refresh_failed(&mut self, error: String) {
        if self.quotas.is_empty() {
            self.connection = ConnectionStatus::Error;
        } else {
            self.connection = ConnectionStatus::Connected;
        }
        self.last_updated_at = Some("Update failed".to_string());
        self.error_message = Some(error);
    }

    /// 兼容旧的扁平化访问接口
    pub fn display_name(&self) -> &str {
        &self.metadata.display_name
    }

    pub fn icon_asset(&self) -> &str {
        &self.metadata.icon_asset
    }

    pub fn dashboard_url(&self) -> &str {
        &self.metadata.dashboard_url
    }

    pub fn brand_name(&self) -> &str {
        &self.metadata.brand_name
    }

    pub fn account_hint(&self) -> &str {
        &self.metadata.account_hint
    }

    pub fn source_label(&self) -> &str {
        &self.metadata.source_label
    }

    /// 格式化上次刷新的相对时间
    pub fn format_last_updated(&self) -> String {
        if let Some(instant) = self.last_refreshed_instant {
            let secs = instant.elapsed().as_secs();
            if secs < 60 {
                "Updated just now".to_string()
            } else if secs < 3600 {
                format!("Updated {} min ago", secs / 60)
            } else {
                format!("Updated {} hr ago", secs / 3600)
            }
        } else if let Some(ref text) = self.last_updated_at {
            text.clone()
        } else {
            match self.connection {
                ConnectionStatus::Connected => "Waiting for data".to_string(),
                ConnectionStatus::Refreshing => "Refreshing…".to_string(),
                ConnectionStatus::Error => "Needs attention".to_string(),
                ConnectionStatus::Disconnected => "Not connected".to_string(),
            }
        }
    }

    /// 获取最高用量的状态等级（用于总览显示）
    #[allow(dead_code)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quota_percentage() {
        let q1 = QuotaInfo::new("test", 50.0, 100.0);
        assert_eq!(q1.percentage(), 50.0);

        let q2 = QuotaInfo::new("test", 150.0, 100.0); // 溢出
        assert_eq!(q2.percentage(), 100.0);

        let q3 = QuotaInfo::new("test", 0.0, 0.0); // 除零
        assert_eq!(q3.percentage(), 0.0);
    }

    #[test]
    fn test_quota_status_level() {
        let q_green = QuotaInfo::new("green", 40.0, 100.0);
        assert_eq!(q_green.status_level(), StatusLevel::Green);

        let q_yellow_edge = QuotaInfo::new("yellow", 50.0, 100.0);
        assert_eq!(q_yellow_edge.status_level(), StatusLevel::Yellow);

        let q_yellow_20 = QuotaInfo::new("yellow", 80.0, 100.0);
        assert_eq!(q_yellow_20.status_level(), StatusLevel::Yellow);

        let q_red = QuotaInfo::new("red", 81.0, 100.0);
        assert_eq!(q_red.status_level(), StatusLevel::Red);
    }

    #[test]
    fn test_status_level_ordering() {
        assert!(StatusLevel::Green < StatusLevel::Yellow);
        assert!(StatusLevel::Yellow < StatusLevel::Red);
        assert_eq!(
            [StatusLevel::Red, StatusLevel::Green, StatusLevel::Yellow]
                .iter()
                .max(),
            Some(&StatusLevel::Red)
        );
    }
}
