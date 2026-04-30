use serde::{Deserialize, Serialize};

use super::QuotaType;

/// 配额标题的展示语义。
///
/// Provider 负责解释原始响应，selector/UI 再基于当前 locale 生成最终文案。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuotaLabelSpec {
    /// 上游或用户自定义原文，保持原样展示
    Raw(String),
    Daily,
    Session,
    Weekly,
    WeeklyModel {
        model: String,
    },
    /// 周配额 + 套餐层级（如 Kimi 的 Moderato）
    WeeklyTier {
        tier: String,
    },
    MonthlyCredits,
    Credits,
    BonusCredits,
    ExtraUsage,
    PremiumRequests {
        plan: String,
    },
    ChatCompletions {
        plan: String,
    },
    MonthlyTier {
        tier: String,
    },
    OnDemand,
    Team,
}

impl QuotaLabelSpec {
    /// 语言无关的稳定 key，用于设置持久化与 UI identity。
    pub fn stable_key(&self, quota_type: &QuotaType) -> String {
        match self {
            Self::Raw(label) => slugify_key(label),
            Self::Daily => "daily".into(),
            Self::Session => "session".into(),
            Self::Weekly | Self::WeeklyTier { .. } => "weekly".into(),
            Self::WeeklyModel { model } => format!("model:{model}"),
            Self::MonthlyCredits => "monthly-credits".into(),
            Self::Credits => {
                match quota_type {
                    QuotaType::Credit => "credit".into(),
                    // 历史兼容：Kiro Regular Credits 早期为 `General` 类型，
                    // stable_key 为 `"general"`；后续改为 `Points` 以修正显示。
                    // 这里保留 `"general"` 以兼容老版本设置中的 hidden_quotas。
                    QuotaType::Points => "general".into(),
                    _ => quota_type.stable_key(),
                }
            }
            Self::BonusCredits => "bonus-credits".into(),
            Self::ExtraUsage => "extra-usage".into(),
            Self::PremiumRequests { .. } => "premium-requests".into(),
            Self::ChatCompletions { .. } => "chat-completions".into(),
            Self::MonthlyTier { .. } => "monthly-tier".into(),
            Self::OnDemand => "on-demand".into(),
            Self::Team => "team".into(),
        }
    }
}

impl From<String> for QuotaLabelSpec {
    fn from(value: String) -> Self {
        Self::Raw(value)
    }
}

impl From<&str> for QuotaLabelSpec {
    fn from(value: &str) -> Self {
        Self::Raw(value.to_string())
    }
}

impl From<&String> for QuotaLabelSpec {
    fn from(value: &String) -> Self {
        Self::Raw(value.clone())
    }
}

/// 配额详情的展示语义（卡片第四行）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum QuotaDetailSpec {
    /// 上游或用户自定义原文，保持原样展示
    Raw(String),
    Unlimited,
    RequestCount {
        used: u32,
        total: u32,
    },
    CreditRemaining {
        remaining: f64,
        total: f64,
    },
    /// 重置时间戳（selector 按当前 locale 格式化倒计时）
    ResetAt {
        epoch_secs: i64,
    },
    /// 仅有日期文本，外壳文案由 selector 本地化
    ResetDate {
        date: String,
    },
    ExpiresInDays {
        days: u32,
    },
}

pub(crate) fn slugify_key(raw: &str) -> String {
    let mut key = String::new();
    let mut last_was_sep = false;
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            key.push(ch.to_ascii_lowercase());
            last_was_sep = false;
        } else if !last_was_sep {
            key.push('-');
            last_was_sep = true;
        }
    }
    let key = key.trim_matches('-');
    if key.is_empty() {
        "general".to_string()
    } else {
        key.to_string()
    }
}
