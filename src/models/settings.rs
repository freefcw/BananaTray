use super::provider::ProviderKind;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// 应用设置
// ============================================================================

/// 应用主题
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppTheme {
    Light,
    Dark,
}

/// Provider 特定配置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderSettings {
    /// Copilot: GitHub Token (Classic PAT with copilot scope)
    pub github_token: Option<String>,
}

/// 应用配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub theme: AppTheme,
    /// 自动刷新间隔（分钟），0 表示禁用自动刷新
    pub refresh_interval_mins: u64,
    pub global_hotkey: String,
    pub auto_hide_window: bool,
    pub visible_provider_count: usize,
    /// 开机自启动
    #[serde(default)]
    pub start_at_login: bool,
    /// 显示消费概览
    #[serde(default = "default_true")]
    pub show_cost_summary: bool,
    /// 检查 Provider 状态页
    #[serde(default = "default_true")]
    pub check_provider_status: bool,
    /// Session 配额变更通知
    #[serde(default = "default_true")]
    pub session_quota_notifications: bool,
    /// Provider 特定配置
    pub providers: ProviderSettings,
    /// 各 Provider 启用状态（key = provider id_key, value = enabled）
    #[serde(default)]
    pub enabled_providers: HashMap<String, bool>,
    /// Provider 在导航栏中的排列顺序（存储 id_key 列表）
    #[serde(default)]
    pub provider_order: Vec<String>,
}

fn default_true() -> bool {
    true
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: AppTheme::Light,
            refresh_interval_mins: 5,
            global_hotkey: "Cmd+Shift+S".to_string(),
            auto_hide_window: true,
            visible_provider_count: 4,
            start_at_login: false,
            show_cost_summary: true,
            check_provider_status: true,
            session_quota_notifications: true,
            providers: ProviderSettings::default(),
            enabled_providers: HashMap::new(),
            provider_order: Vec::new(),
        }
    }
}

impl AppSettings {
    /// 检查指定 Provider 是否已启用
    pub fn is_provider_enabled(&self, kind: ProviderKind) -> bool {
        self.enabled_providers
            .get(&kind.id_key())
            .copied()
            .unwrap_or(false)
    }

    /// 设置指定 Provider 的启用状态
    pub fn set_provider_enabled(&mut self, kind: ProviderKind, enabled: bool) {
        self.enabled_providers.insert(kind.id_key(), enabled);
    }

    /// 按用户自定义顺序返回所有 Provider。未在 provider_order 中出现的 Provider 追加到末尾。
    pub fn ordered_providers(&self) -> Vec<ProviderKind> {
        let all = ProviderKind::all();
        let mut result: Vec<ProviderKind> = Vec::with_capacity(all.len());

        // 先按保存的顺序添加
        for key in &self.provider_order {
            if let Some(kind) = all.iter().find(|k| k.id_key() == key.as_str()) {
                if !result.contains(kind) {
                    result.push(*kind);
                }
            }
        }

        // 再追加未出现的 Provider（保持默认顺序）
        for kind in all {
            if !result.contains(kind) {
                result.push(*kind);
            }
        }

        result
    }

    /// 将指定 Provider 在排序中上移一位。返回 true 表示发生了移动。
    pub fn move_provider_up(&mut self, kind: ProviderKind) -> bool {
        self.ensure_provider_order();
        let key = kind.id_key();
        if let Some(pos) = self.provider_order.iter().position(|k| k == &key) {
            if pos > 0 {
                self.provider_order.swap(pos, pos - 1);
                return true;
            }
        }
        false
    }

    /// 将指定 Provider 在排序中下移一位。返回 true 表示发生了移动。
    pub fn move_provider_down(&mut self, kind: ProviderKind) -> bool {
        self.ensure_provider_order();
        let key = kind.id_key();
        if let Some(pos) = self.provider_order.iter().position(|k| k == &key) {
            if pos + 1 < self.provider_order.len() {
                self.provider_order.swap(pos, pos + 1);
                return true;
            }
        }
        false
    }

    /// 确保 provider_order 包含所有 Provider
    fn ensure_provider_order(&mut self) {
        if self.provider_order.is_empty() {
            self.provider_order = ProviderKind::all().iter().map(|k| k.id_key()).collect();
        } else {
            // 补全缺失的 Provider
            for kind in ProviderKind::all() {
                let key = kind.id_key();
                if !self.provider_order.contains(&key) {
                    self.provider_order.push(key);
                }
            }
        }
    }
}
