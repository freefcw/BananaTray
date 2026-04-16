//! ProviderConfig — 设置页 Sidebar 管理
//!
//! 管理设置页 sidebar 列表：默认值填充、增删、可添加项查询。

use super::*;

impl ProviderConfig {
    /// 设置页 sidebar 应展示的 Provider ID 列表。
    ///
    /// 返回 `sidebar_providers` 中有效的 Provider，按 `provider_order` 排序；
    /// 不在 `sidebar_providers` 中的项不展示。
    pub fn sidebar_provider_ids(&self, custom_ids: &[ProviderId]) -> Vec<ProviderId> {
        let sidebar_set: HashSet<&str> =
            self.sidebar_providers.iter().map(|s| s.as_str()).collect();
        // 按 provider_order 的顺序，过滤出在 sidebar 中的项
        self.ordered_provider_ids(custom_ids)
            .into_iter()
            .filter(|id| sidebar_set.contains(id.id_key().as_str()))
            .collect()
    }

    /// 返回可添加到 sidebar 的内置 Provider 列表。
    ///
    /// 规则：全量内置 Provider 中排除已在 sidebar 中的（Custom 类型不在此列，
    /// NewAPI 有独立入口）。
    pub fn addable_provider_kinds(&self) -> Vec<ProviderKind> {
        let sidebar_set: HashSet<&str> =
            self.sidebar_providers.iter().map(|s| s.as_str()).collect();
        ProviderKind::all()
            .iter()
            .filter(|kind| !sidebar_set.contains(kind.id_key()))
            .copied()
            .collect()
    }

    /// 将 Provider 添加到 sidebar 列表。
    ///
    /// 内置 Provider 重复添加返回 false；Custom 类型始终允许。
    pub fn add_to_sidebar(&mut self, id: &ProviderId) -> bool {
        let key = id.id_key();
        // 内置 Provider 去重
        if id.is_builtin() && self.sidebar_providers.contains(&key) {
            return false;
        }
        self.sidebar_providers.push(key.clone());
        // 同步到 provider_order（排序列表也需要包含该项）
        if !self.provider_order.contains(&key) {
            self.provider_order.push(key);
        }
        true
    }

    /// 从 sidebar 列表移除 Provider。返回 true 表示移除成功。
    pub fn remove_from_sidebar(&mut self, id: &ProviderId) -> bool {
        let key = id.id_key();
        let before = self.sidebar_providers.len();
        self.sidebar_providers.retain(|k| *k != key);
        self.sidebar_providers.len() != before
    }
}
