use super::{QuotaInfo, RateLimitResetSnapshot};

/// Provider 刷新返回的完整数据
#[derive(Debug, Clone)]
pub struct RefreshData {
    /// 配额信息列表
    pub quotas: Vec<QuotaInfo>,
    /// 账户邮箱（可选）
    pub account_email: Option<String>,
    /// 账户套餐等级（可选）
    pub account_tier: Option<String>,
    /// 本次刷新实际使用的数据源（可选，覆盖静态 metadata.source_label）
    pub source_label: Option<String>,
    /// 可兑换的限流重置。不是配额，不参与历史采样和额度告警。
    pub rate_limit_resets: RateLimitResetSnapshot,
}

impl RefreshData {
    /// 仅包含配额信息
    pub fn quotas_only(quotas: Vec<QuotaInfo>) -> Self {
        Self {
            quotas,
            account_email: None,
            account_tier: None,
            source_label: None,
            rate_limit_resets: RateLimitResetSnapshot::default(),
        }
    }

    /// 包含完整信息
    pub fn with_account(
        quotas: Vec<QuotaInfo>,
        account_email: Option<String>,
        account_tier: Option<String>,
    ) -> Self {
        Self {
            quotas,
            account_email,
            account_tier,
            source_label: None,
            rate_limit_resets: RateLimitResetSnapshot::default(),
        }
    }

    /// 附加上游返回的限流重置。空快照表示这次没有可展示的重置。
    pub fn with_rate_limit_resets(mut self, resets: RateLimitResetSnapshot) -> Self {
        self.rate_limit_resets = resets;
        self
    }

    /// 附加本次刷新实际使用的数据源标签。
    pub fn with_source_label(mut self, source_label: impl Into<String>) -> Self {
        self.source_label = Some(source_label.into());
        self
    }
}
