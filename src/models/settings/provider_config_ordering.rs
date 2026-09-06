//! ProviderConfig — Provider 排序管理
//!
//! 负责有序 `provider_layout` 的读取、补齐和拖拽排序；数组位置就是唯一排序来源。

use super::*;

impl ProviderConfig {
    /// 按用户布局顺序返回所有内置 Provider。未出现在布局中的 Provider 追加到末尾。
    pub fn ordered_providers(&self) -> Vec<ProviderKind> {
        self.ordered_provider_ids(&[])
            .into_iter()
            .filter_map(|id| id.as_builtin())
            .collect()
    }

    /// 按用户布局顺序返回所有 Provider（内置 + 已发现的自定义 Provider）。
    pub fn ordered_provider_ids(&self, custom_ids: &[ProviderId]) -> Vec<ProviderId> {
        let mut result = Vec::new();
        let mut seen = HashSet::new();

        for item in &self.provider_layout {
            let id = ProviderId::from_id_key(&item.id);
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

    /// 将指定 sidebar Provider 移动到目标可见索引位置（拖拽排序）。
    pub fn move_provider_to_index(
        &mut self,
        id: &ProviderId,
        target_index: usize,
        custom_ids: &[ProviderId],
    ) -> bool {
        self.ensure_layout_items(custom_ids);
        let Some(current) = self
            .provider_layout
            .iter()
            .position(|item| item.id == id.id_key() && item.in_sidebar)
        else {
            return false;
        };

        let current_visible_index = self.provider_layout[..current]
            .iter()
            .filter(|candidate| candidate.in_sidebar)
            .count();
        let item = self.provider_layout.remove(current);
        let visible_count = self
            .provider_layout
            .iter()
            .filter(|candidate| candidate.in_sidebar)
            .count();
        let target_index = target_index.min(visible_count);
        if current_visible_index == target_index {
            self.provider_layout
                .insert(current.min(self.provider_layout.len()), item);
            return false;
        }

        let target_position = if target_index == visible_count {
            self.provider_layout
                .iter()
                .rposition(|candidate| candidate.in_sidebar)
                .map(|position| position + 1)
                .unwrap_or(self.provider_layout.len())
        } else {
            self.provider_layout
                .iter()
                .enumerate()
                .filter(|(_, candidate)| candidate.in_sidebar)
                .nth(target_index)
                .map(|(position, _)| position)
                .unwrap_or(self.provider_layout.len())
        };

        self.provider_layout.insert(target_position, item);
        true
    }

    fn ensure_layout_items(&mut self, custom_ids: &[ProviderId]) {
        for &kind in ProviderKind::all() {
            self.ensure_layout_item(&ProviderId::BuiltIn(kind));
        }
        for custom_id in custom_ids {
            self.ensure_layout_item(custom_id);
        }
        self.normalize_layout();
    }
}
