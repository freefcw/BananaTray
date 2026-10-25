//! ProviderConfig — 配额可见性管理
//!
//! 控制哪些配额在托盘弹窗中可见/隐藏。隐藏的配额同时退出状态颜色和告警判定。

use super::*;
use crate::models::{QuotaLabelSpec, StatusLevel};

impl ProviderConfig {
    /// 判断某个 quota 是否在托盘弹窗中可见（未被隐藏）
    /// `quota_key` 应使用 `QuotaInfo::stable_key`，而非 i18n label
    pub fn is_quota_visible(&self, provider_id: &ProviderId, quota_key: &str) -> bool {
        self.hidden_quotas
            .get(&provider_id.id_key())
            .is_none_or(|set| !set.contains(quota_key))
    }

    /// 统计可见配额数量
    pub fn visible_quota_count(&self, provider_id: &ProviderId, quotas: &[QuotaInfo]) -> usize {
        quotas
            .iter()
            .filter(|q| self.is_quota_visible(provider_id, &q.stable_key))
            .count()
    }

    /// 过滤出在托盘弹窗中可见的配额
    pub fn visible_quotas<'a>(
        &self,
        provider_id: &ProviderId,
        quotas: &'a [QuotaInfo],
    ) -> Vec<&'a QuotaInfo> {
        quotas
            .iter()
            .filter(|q| self.is_quota_visible(provider_id, &q.stable_key))
            .collect()
    }

    /// 可见配额里最差的状态等级。用户隐藏的配额不参与状态颜色和告警。
    pub fn visible_worst_status(
        &self,
        provider_id: &ProviderId,
        quotas: &[QuotaInfo],
        rules: &QuotaRules,
    ) -> StatusLevel {
        self.visible_quotas(provider_id, quotas)
            .into_iter()
            .map(|q| q.status_level(rules))
            .max()
            .unwrap_or(StatusLevel::Green)
    }

    /// 超额用量余额卡回到正数时自动取消隐藏，返回是否有改动。
    ///
    /// 余额为 0 时用户可以把卡藏起来（不再标红、不再告警）；一旦又有余额，
    /// 这张卡重新有意义，自动回到弹窗并恢复监测。
    pub fn reveal_positive_extra_usage(
        &mut self,
        provider_id: &ProviderId,
        quotas: &[QuotaInfo],
    ) -> bool {
        let Some(hidden) = self.hidden_quotas.get_mut(&provider_id.id_key()) else {
            return false;
        };
        let mut changed = false;
        for quota in quotas {
            if quota.label_spec == QuotaLabelSpec::ExtraUsage
                && quota.remaining_balance.is_some_and(|balance| balance > 0.0)
            {
                changed |= hidden.remove(&quota.stable_key);
            }
        }
        changed
    }

    /// 切换某个 quota 的可见性（隐藏 ↔ 显示）
    pub fn toggle_quota_visibility(&mut self, provider_id: &ProviderId, quota_key: String) {
        let set = self.hidden_quotas.entry(provider_id.id_key()).or_default();
        if !set.remove(&quota_key) {
            set.insert(quota_key);
        }
    }

    /// 用户指定的总览主额度 stable_key。
    ///
    /// 主额度只影响总览展示：折叠态优先展示它而非最差配额。被隐藏或配额已不存在时
    /// 由调用方回退到最差配额，存储值仍然保留。
    pub fn primary_quota_key(&self, provider_id: &ProviderId) -> Option<&str> {
        self.primary_quotas
            .get(&provider_id.id_key())
            .map(String::as_str)
    }

    /// 切换某个 quota 的总览主额度状态（设为主额度 ↔ 恢复最差配额优先）。
    pub fn toggle_primary_quota(&mut self, provider_id: &ProviderId, quota_key: String) {
        let key = provider_id.id_key();
        if self.primary_quotas.get(&key) == Some(&quota_key) {
            self.primary_quotas.remove(&key);
        } else {
            self.primary_quotas.insert(key, quota_key);
        }
    }
}
