//! ProviderConfig — Provider 排序管理
//!
//! 负责 Provider 的用户自定义排列顺序、拖拽排序等逻辑。

use super::*;

impl ProviderConfig {
    /// 按用户自定义顺序返回所有内置 Provider。未在 provider_order 中出现的追加到末尾。
    pub fn ordered_providers(&self) -> Vec<ProviderKind> {
        let mut result = Vec::with_capacity(ProviderKind::all().len());
        let mut seen = HashSet::with_capacity(ProviderKind::all().len());

        for key in &self.provider_order {
            if let Some(kind) = ProviderKind::from_id_key(key) {
                if seen.insert(kind) {
                    result.push(kind);
                }
            }
        }

        for &kind in ProviderKind::all() {
            if seen.insert(kind) {
                result.push(kind);
            }
        }

        result
    }

    /// 按用户自定义顺序返回所有 Provider（内置 + 自定义）。
    pub fn ordered_provider_ids(&self, custom_ids: &[ProviderId]) -> Vec<ProviderId> {
        let mut result = Vec::new();
        let mut seen = HashSet::new();

        for key in &self.provider_order {
            let id = ProviderId::from_id_key(key);
            if seen.insert(id.clone()) {
                result.push(id);
            }
        }

        for &kind in ProviderKind::all() {
            let id = ProviderId::BuiltIn(kind);
            if seen.insert(id.clone()) {
                result.push(id);
            }
        }

        for custom_id in custom_ids {
            if seen.insert(custom_id.clone()) {
                result.push(custom_id.clone());
            }
        }

        result
    }

    /// 将指定 Provider 移动到目标索引位置（拖拽排序）。返回 true 表示发生了移动。
    pub fn move_provider_to_index(
        &mut self,
        id: &ProviderId,
        target_index: usize,
        custom_ids: &[ProviderId],
    ) -> bool {
        self.ensure_order(custom_ids);
        let key = id.id_key();
        if let Some(current) = self.provider_order.iter().position(|k| *k == key) {
            let target = target_index.min(self.provider_order.len().saturating_sub(1));
            if current != target {
                let item = self.provider_order.remove(current);
                self.provider_order.insert(target, item);
                return true;
            }
        }
        false
    }

    /// 确保 provider_order 包含所有 Provider（内置 + 自定义）
    fn ensure_order(&mut self, custom_ids: &[ProviderId]) {
        self.provider_order = self
            .ordered_provider_ids(custom_ids)
            .into_iter()
            .map(|id| id.id_key().to_string())
            .collect();
    }
}
