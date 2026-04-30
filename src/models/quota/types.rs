use serde::{Deserialize, Serialize};

/// 配额类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuotaType {
    /// 5h 滑动窗口会话配额
    Session,
    /// 周配额（所有模型合计）
    Weekly,
    /// 按模型的周配额（如 Opus / Sonnet）
    ModelSpecific(String),
    /// 基于金额的信用额度（显示带 `$` 前缀）
    Credit,
    /// 基于积分的配额（非货币）。
    ///
    /// 与 `Credit` 区别：显示为绝对数值（如 `12.39 / 50.00`），不带货币符号。
    /// 与 `General` 区别：不会被 `is_percentage_mode()` 误判为百分比。
    /// 注意：状态颜色阈值（Green / Yellow / Red）仍按 `percent_remaining()` 计算，
    /// 只有显示文本走专属分支。
    Points,
    /// 通用/不确定类型
    General,
}

impl QuotaType {
    /// 语言无关的稳定标识符，用于配置持久化（hidden_quotas key）。
    /// 不依赖 i18n，切换语言不会导致配置失效。
    pub fn stable_key(&self) -> String {
        match self {
            QuotaType::Session => "session".into(),
            QuotaType::Weekly => "weekly".into(),
            QuotaType::ModelSpecific(model) => format!("model:{model}"),
            QuotaType::Credit => "credit".into(),
            QuotaType::Points => "points".into(),
            QuotaType::General => "general".into(),
        }
    }
}

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
