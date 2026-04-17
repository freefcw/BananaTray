//! ProviderConfig — 配额可见性管理
//!
//! 控制哪些配额在托盘弹窗中可见/隐藏。

use super::*;

impl ProviderConfig {
    /// 判断某个 quota 是否在托盘弹窗中可见（未被隐藏）
    /// `quota_key` 应使用 `QuotaInfo::stable_key`，而非 i18n label
    pub fn is_quota_visible(&self, kind: ProviderKind, quota_key: &str) -> bool {
        self.hidden_quotas
            .get(kind.id_key())
            .is_none_or(|set| !set.contains(quota_key))
    }

    /// 统计可见配额数量
    pub fn visible_quota_count(&self, kind: ProviderKind, quotas: &[QuotaInfo]) -> usize {
        quotas
            .iter()
            .filter(|q| self.is_quota_visible(kind, &q.stable_key))
            .count()
    }

    /// 过滤出在托盘弹窗中可见的配额
    pub fn visible_quotas<'a>(
        &self,
        kind: ProviderKind,
        quotas: &'a [QuotaInfo],
    ) -> Vec<&'a QuotaInfo> {
        quotas
            .iter()
            .filter(|q| self.is_quota_visible(kind, &q.stable_key))
            .collect()
    }

    /// 切换某个 quota 的可见性（隐藏 ↔ 显示）
    pub fn toggle_quota_visibility(&mut self, kind: ProviderKind, quota_key: String) {
        let set = self
            .hidden_quotas
            .entry(kind.id_key().to_string())
            .or_default();
        if !set.remove(&quota_key) {
            set.insert(quota_key);
        }
    }
}
