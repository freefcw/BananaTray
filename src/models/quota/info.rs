use serde::{Deserialize, Serialize};

use super::{slugify_key, QuotaDetailSpec, QuotaLabelSpec, QuotaType, StatusLevel};

/// 用量配额信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaInfo {
    /// 已使用量
    pub used: f64,
    /// 总配额
    pub limit: f64,
    /// 配额类型
    #[serde(default = "default_quota_type")]
    pub quota_type: QuotaType,
    /// 配额 identity（语言无关，用于设置持久化、UI key、动画 key）
    #[serde(default)]
    pub stable_key: String,
    /// 标题展示语义
    #[serde(default = "default_quota_label_spec")]
    pub label_spec: QuotaLabelSpec,
    /// 第四行详情展示语义
    pub detail_spec: Option<QuotaDetailSpec>,
    /// 余额模式：直接存储剩余额度值（与 used/limit 进度条模式互斥）
    #[serde(default)]
    pub remaining_balance: Option<f64>,
}

fn default_quota_type() -> QuotaType {
    QuotaType::General
}

fn default_quota_label_spec() -> QuotaLabelSpec {
    QuotaLabelSpec::Raw("quota".to_string())
}
impl QuotaInfo {
    pub fn new(label: impl Into<String>, used: f64, limit: f64) -> Self {
        let label = label.into();
        Self {
            used,
            limit,
            quota_type: QuotaType::General,
            stable_key: slugify_key(&label),
            label_spec: QuotaLabelSpec::Raw(label),
            detail_spec: None,
            remaining_balance: None,
        }
    }

    /// 创建带完整信息的配额
    pub fn with_details(
        label: impl Into<QuotaLabelSpec>,
        used: f64,
        limit: f64,
        quota_type: QuotaType,
        detail_spec: Option<QuotaDetailSpec>,
    ) -> Self {
        let label_spec = label.into();
        let stable_key = label_spec.stable_key(&quota_type);
        Self {
            used,
            limit,
            quota_type,
            stable_key,
            label_spec,
            detail_spec,
            remaining_balance: None,
        }
    }

    /// 创建带显式 key 的配额
    pub fn with_key(
        stable_key: impl Into<String>,
        label: impl Into<QuotaLabelSpec>,
        used: f64,
        limit: f64,
        quota_type: QuotaType,
        detail_spec: Option<QuotaDetailSpec>,
    ) -> Self {
        Self {
            used,
            limit,
            quota_type,
            stable_key: stable_key.into(),
            label_spec: label.into(),
            detail_spec,
            remaining_balance: None,
        }
    }

    /// 创建余额模式的配额（无进度条，仅展示余额和已用）
    pub fn balance_only(
        label: impl Into<QuotaLabelSpec>,
        remaining: f64,
        used: Option<f64>,
        quota_type: QuotaType,
        detail_spec: Option<QuotaDetailSpec>,
    ) -> Self {
        let label_spec = label.into();
        let stable_key = label_spec.stable_key(&quota_type);
        Self {
            used: used.unwrap_or(0.0),
            limit: 0.0,
            quota_type,
            stable_key,
            label_spec,
            detail_spec,
            remaining_balance: Some(remaining),
        }
    }

    /// 创建带显式 key 的余额模式配额
    pub fn balance_only_with_key(
        stable_key: impl Into<String>,
        label: impl Into<QuotaLabelSpec>,
        remaining: f64,
        used: Option<f64>,
        quota_type: QuotaType,
        detail_spec: Option<QuotaDetailSpec>,
    ) -> Self {
        Self {
            used: used.unwrap_or(0.0),
            limit: 0.0,
            quota_type,
            stable_key: stable_key.into(),
            label_spec: label.into(),
            detail_spec,
            remaining_balance: Some(remaining),
        }
    }

    /// 是否为余额模式（无进度条）
    pub fn is_balance_only(&self) -> bool {
        self.remaining_balance.is_some()
    }

    /// 使用百分比 (可负数，当超出配额时)
    pub fn percentage(&self) -> f64 {
        if self.limit <= 0.0 {
            return 0.0;
        }
        // 不 clamp，允许负数（超出配额的情况）
        self.used / self.limit * 100.0
    }

    /// 剩余百分比 (可负数，当超出配额时)
    pub fn percent_remaining(&self) -> f64 {
        if self.limit <= 0.0 {
            return 0.0;
        }
        // 不 clamp，允许负数（超出配额的情况）
        (self.limit - self.used) / self.limit * 100.0
    }

    /// 是否是纯百分比模式（limit == 100.0，数据本身就是百分比）
    #[allow(dead_code)]
    pub fn is_percentage_mode(&self) -> bool {
        (self.limit - 100.0).abs() < 1e-9
    }

    // ========================================================================
    // 状态判断（基于 status_level 单一真理来源）
    // ========================================================================

    /// 状态等级：Green / Yellow / Red
    ///
    /// 传统模式阈值（基于百分比）：
    /// - Green: 剩余 > 50%
    /// - Yellow: 剩余 20% ~ 50%（包含边界）
    /// - Red: 剩余 < 20%
    ///
    /// 余额模式阈值（基于绝对值，仅 Credit 类型）：
    /// - Green: 余额 >= $5
    /// - Yellow: $1 ~ $5
    /// - Red: < $1
    pub fn status_level(&self) -> StatusLevel {
        if let Some(balance) = self.remaining_balance {
            // 余额模式：按绝对值判断
            if balance >= 5.0 {
                StatusLevel::Green
            } else if balance >= 1.0 {
                StatusLevel::Yellow
            } else {
                StatusLevel::Red
            }
        } else {
            // 传统模式：按百分比判断
            let remaining_pct = self.percent_remaining();
            if remaining_pct > 50.0 {
                StatusLevel::Green
            } else if remaining_pct >= 20.0 {
                StatusLevel::Yellow
            } else {
                StatusLevel::Red
            }
        }
    }

    /// 是否已耗尽（已使用 >= 配额）
    pub fn is_depleted(&self) -> bool {
        self.used >= self.limit && self.limit > 0.0
    }

    /// 是否健康（Green 状态）
    pub fn is_healthy(&self) -> bool {
        self.status_level() == StatusLevel::Green
    }

    /// 是否需要警告（Yellow 状态）
    pub fn is_warning(&self) -> bool {
        self.status_level() == StatusLevel::Yellow
    }

    /// 是否紧急（Red 状态且未耗尽）
    pub fn is_critical(&self) -> bool {
        self.status_level() == StatusLevel::Red && !self.is_depleted()
    }

    // ========================================================================
    // 类型判断
    // ========================================================================

    /// 按配额类型查找会话配额
    pub fn is_session(&self) -> bool {
        self.quota_type == QuotaType::Session
    }

    /// 按配额类型查找周配额
    pub fn is_weekly(&self) -> bool {
        self.quota_type == QuotaType::Weekly
    }

    /// 按配额类型查找信用配额
    pub fn is_credit(&self) -> bool {
        self.quota_type == QuotaType::Credit
    }
}
