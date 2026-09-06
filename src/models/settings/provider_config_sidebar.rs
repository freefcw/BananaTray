//! ProviderConfig — 设置页 Sidebar 管理
//!
//! 管理统一 `provider_layout` 中的 sidebar 可见性、默认 Provider 和自定义 Provider 发现。

use super::*;

impl ProviderConfig {
    /// 返回按布局顺序排列的 sidebar Provider。
    pub fn sidebar_provider_ids(&self, custom_ids: &[ProviderId]) -> Vec<ProviderId> {
        self.ordered_provider_ids(custom_ids)
            .into_iter()
            .filter(|id| self.is_in_sidebar(id))
            .collect()
    }

    /// 返回可添加到 sidebar 的内置 Provider 列表。
    pub fn addable_provider_kinds(&self) -> Vec<ProviderKind> {
        ProviderKind::all()
            .iter()
            .filter(|kind| !self.is_in_sidebar(&ProviderId::BuiltIn(**kind)))
            .copied()
            .collect()
    }

    /// 自动登记首次发现的自定义 Provider。
    ///
    /// 没有任何既有布局记录时，新发现的 custom Provider 自动加入 sidebar 并启用；
    /// 已存在但被用户禁用或移除的记录保持原状态。
    pub fn register_discovered_custom_providers(&mut self, ids: &[ProviderId]) -> Vec<ProviderId> {
        let mut registered = Vec::new();

        for id in ids.iter().filter(|id| id.is_custom()) {
            if self.layout_item(id).is_some() {
                continue;
            }

            let item = self.ensure_layout_item(id);
            item.in_sidebar = true;
            item.enabled = true;
            registered.push(id.clone());
        }

        registered
    }

    /// 将 Provider 添加到 sidebar；保留其原有排序位置和启用状态。
    pub fn add_to_sidebar(&mut self, id: &ProviderId) -> bool {
        let item = self.ensure_layout_item(id);
        if item.in_sidebar {
            return false;
        }
        item.in_sidebar = true;
        true
    }

    /// 从 sidebar 移除 Provider，同时禁用它但保留布局项以记住排序位置。
    pub fn remove_from_sidebar(&mut self, id: &ProviderId) -> bool {
        let Some(item) = self.layout_item_mut(id) else {
            return false;
        };
        if !item.in_sidebar {
            return false;
        }
        item.in_sidebar = false;
        item.enabled = false;
        true
    }
}
