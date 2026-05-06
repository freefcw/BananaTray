//! Quota alert domain types and transition tracking.

use crate::models::{ProviderId, QuotaInfo};
use log::info;
use std::collections::HashMap;

/// Provider 配额的告警状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AlertState {
    /// 余量充足（> 10%）
    Normal,
    /// 余量不足（≤ 10%，> 0%）
    Low,
    /// 余量耗尽（= 0%）
    Exhausted,
}

/// 应该发送的告警通知类型
#[derive(Debug, Clone, PartialEq)]
pub enum QuotaAlert {
    /// 余量不足 10%
    LowQuota {
        provider_name: String,
        remaining_pct: f64,
    },
    /// 余额已耗尽
    Exhausted { provider_name: String },
    /// 配额已恢复（从耗尽状态）
    Recovered {
        provider_name: String,
        remaining_pct: f64,
    },
}

impl AlertState {
    /// 根据剩余百分比确定目标状态
    fn from_remaining(remaining_pct: f64) -> Self {
        if remaining_pct <= 0.0 {
            Self::Exhausted
        } else if remaining_pct <= 10.0 {
            Self::Low
        } else {
            Self::Normal
        }
    }
}

/// 追踪每个 Provider 的配额告警状态，检测状态转换并产生告警事件。
///
/// 设计为应用层纯业务组件：只输出“应该发什么通知”，不关心具体 OS 发送方式。
#[derive(Default)]
pub struct QuotaAlertTracker {
    states: HashMap<ProviderId, AlertState>,
}

impl QuotaAlertTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// 根据最新的 quotas 数据更新 Provider 状态，返回可能需要发送的告警。
    ///
    /// 判定逻辑：取所有 quota 中最差的剩余百分比代表整个 Provider。
    pub fn update(
        &mut self,
        id: &ProviderId,
        provider_name: &str,
        quotas: &[QuotaInfo],
    ) -> Option<QuotaAlert> {
        if quotas.is_empty() {
            return None;
        }

        // 计算所有 quota 中最差（最小）的剩余百分比
        // 注意：不能直接用 q.percent_remaining()，因为 balance_only 配额 limit=0，
        // percent_remaining() 返回 0.0 会导致误报。balance_only 配额使用独立的
        // status_level() 绝对值逻辑（$5/$1 阈值），这里用 100-percentage 使其不影响排序。
        let worst_remaining = quotas
            .iter()
            .map(|q| {
                let pct = q.percentage();
                (100.0 - pct).max(0.0)
            })
            .fold(f64::MAX, f64::min);

        let new_state = AlertState::from_remaining(worst_remaining);

        // 首次数据只建立基线，不触发告警（避免启动时误报）
        let Some(&old_state) = self.states.get(id) else {
            self.states.insert(id.clone(), new_state);
            return None;
        };

        // 更新状态
        self.states.insert(id.clone(), new_state);

        // 状态未变化，不触发
        if old_state == new_state {
            return None;
        }

        let name = provider_name.to_string();
        match (old_state, new_state) {
            // 进入 Low 状态
            (AlertState::Normal, AlertState::Low) => {
                info!(
                    target: "notification",
                    "{} quota low: {:.1}% remaining",
                    name,
                    worst_remaining
                );
                Some(QuotaAlert::LowQuota {
                    provider_name: name,
                    remaining_pct: worst_remaining,
                })
            }
            // 进入 Exhausted 状态
            (_, AlertState::Exhausted) => {
                info!(target: "notification", "{} quota exhausted", name);
                Some(QuotaAlert::Exhausted {
                    provider_name: name,
                })
            }
            // 从 Exhausted 恢复
            (AlertState::Exhausted, _) => {
                info!(
                    target: "notification",
                    "{} quota recovered: {:.1}% remaining",
                    name,
                    worst_remaining
                );
                Some(QuotaAlert::Recovered {
                    provider_name: name,
                    remaining_pct: worst_remaining,
                })
            }
            // 其他转换不触发通知
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ProviderKind, QuotaInfo};

    fn pid(kind: ProviderKind) -> ProviderId {
        ProviderId::BuiltIn(kind)
    }

    fn make_quota(used: f64, limit: f64) -> QuotaInfo {
        QuotaInfo::new("test", used, limit)
    }

    #[test]
    fn test_alert_state_from_remaining() {
        assert_eq!(AlertState::from_remaining(50.0), AlertState::Normal);
        assert_eq!(AlertState::from_remaining(10.0), AlertState::Low);
        assert_eq!(AlertState::from_remaining(5.0), AlertState::Low);
        assert_eq!(AlertState::from_remaining(0.0), AlertState::Exhausted);
    }

    #[test]
    fn test_no_alert_on_first_normal_data() {
        let mut tracker = QuotaAlertTracker::new();
        let quotas = vec![make_quota(30.0, 100.0)];
        let alert = tracker.update(&pid(ProviderKind::Claude), "Claude", &quotas);
        assert!(alert.is_none(), "首次正常数据不应触发告警");
    }

    #[test]
    fn test_normal_to_low() {
        let mut tracker = QuotaAlertTracker::new();
        let normal = vec![make_quota(30.0, 100.0)];
        tracker.update(&pid(ProviderKind::Claude), "Claude", &normal);

        let low = vec![make_quota(92.0, 100.0)];
        let alert = tracker.update(&pid(ProviderKind::Claude), "Claude", &low);
        assert!(matches!(alert, Some(QuotaAlert::LowQuota { .. })));
    }

    #[test]
    fn test_low_to_exhausted() {
        let mut tracker = QuotaAlertTracker::new();
        let normal = vec![make_quota(30.0, 100.0)];
        tracker.update(&pid(ProviderKind::Claude), "Claude", &normal);

        let low = vec![make_quota(95.0, 100.0)];
        tracker.update(&pid(ProviderKind::Claude), "Claude", &low);

        let exhausted = vec![make_quota(100.0, 100.0)];
        let alert = tracker.update(&pid(ProviderKind::Claude), "Claude", &exhausted);
        assert!(matches!(alert, Some(QuotaAlert::Exhausted { .. })));
    }

    #[test]
    fn test_normal_to_exhausted_directly() {
        let mut tracker = QuotaAlertTracker::new();
        let normal = vec![make_quota(30.0, 100.0)];
        tracker.update(&pid(ProviderKind::Claude), "Claude", &normal);

        let exhausted = vec![make_quota(100.0, 100.0)];
        let alert = tracker.update(&pid(ProviderKind::Claude), "Claude", &exhausted);
        assert!(matches!(alert, Some(QuotaAlert::Exhausted { .. })));
    }

    #[test]
    fn test_exhausted_to_recovery() {
        let mut tracker = QuotaAlertTracker::new();
        let normal = vec![make_quota(30.0, 100.0)];
        tracker.update(&pid(ProviderKind::Claude), "Claude", &normal);

        let exhausted = vec![make_quota(100.0, 100.0)];
        tracker.update(&pid(ProviderKind::Claude), "Claude", &exhausted);

        let recovered = vec![make_quota(50.0, 100.0)];
        let alert = tracker.update(&pid(ProviderKind::Claude), "Claude", &recovered);
        assert!(matches!(alert, Some(QuotaAlert::Recovered { .. })));
    }

    #[test]
    fn test_exhausted_to_low_still_recovers() {
        let mut tracker = QuotaAlertTracker::new();
        let normal = vec![make_quota(30.0, 100.0)];
        tracker.update(&pid(ProviderKind::Claude), "Claude", &normal);

        let exhausted = vec![make_quota(100.0, 100.0)];
        tracker.update(&pid(ProviderKind::Claude), "Claude", &exhausted);

        let low = vec![make_quota(95.0, 100.0)];
        let alert = tracker.update(&pid(ProviderKind::Claude), "Claude", &low);
        assert!(
            matches!(alert, Some(QuotaAlert::Recovered { .. })),
            "从耗尽恢复到 Low 也应触发恢复通知"
        );
    }

    #[test]
    fn test_repeated_state_no_alert() {
        let mut tracker = QuotaAlertTracker::new();
        let normal = vec![make_quota(30.0, 100.0)];
        tracker.update(&pid(ProviderKind::Claude), "Claude", &normal);

        let low = vec![make_quota(92.0, 100.0)];
        tracker.update(&pid(ProviderKind::Claude), "Claude", &low);

        let still_low = vec![make_quota(93.0, 100.0)];
        let alert = tracker.update(&pid(ProviderKind::Claude), "Claude", &still_low);
        assert!(alert.is_none(), "重复 Low 状态不应重复告警");
    }

    #[test]
    fn test_worst_quota_determines_state() {
        let mut tracker = QuotaAlertTracker::new();
        let normal = vec![make_quota(30.0, 100.0)];
        tracker.update(&pid(ProviderKind::Claude), "Claude", &normal);

        let mixed = vec![make_quota(30.0, 100.0), make_quota(95.0, 100.0)];
        let alert = tracker.update(&pid(ProviderKind::Claude), "Claude", &mixed);
        assert!(
            matches!(alert, Some(QuotaAlert::LowQuota { .. })),
            "应取最差的 quota 决定状态"
        );
    }

    #[test]
    fn test_empty_quotas_no_alert() {
        let mut tracker = QuotaAlertTracker::new();
        let alert = tracker.update(&pid(ProviderKind::Claude), "Claude", &[]);
        assert!(alert.is_none(), "空 quotas 不应触发告警");
    }

    #[test]
    fn test_independent_providers() {
        let mut tracker = QuotaAlertTracker::new();

        let normal = vec![make_quota(30.0, 100.0)];
        tracker.update(&pid(ProviderKind::Claude), "Claude", &normal);
        tracker.update(&pid(ProviderKind::Gemini), "Gemini", &normal);

        let low = vec![make_quota(92.0, 100.0)];
        let alert = tracker.update(&pid(ProviderKind::Claude), "Claude", &low);
        assert!(matches!(alert, Some(QuotaAlert::LowQuota { .. })));

        let still_normal = vec![make_quota(40.0, 100.0)];
        let alert = tracker.update(&pid(ProviderKind::Gemini), "Gemini", &still_normal);
        assert!(alert.is_none(), "Gemini 状态未变，不应触发");
    }

    #[test]
    fn test_first_data_low_no_alert() {
        let mut tracker = QuotaAlertTracker::new();
        let low = vec![make_quota(95.0, 100.0)];
        let alert = tracker.update(&pid(ProviderKind::Claude), "Claude", &low);
        assert!(alert.is_none(), "首次 Low 数据不应触发告警");
    }

    #[test]
    fn test_first_data_exhausted_no_alert() {
        let mut tracker = QuotaAlertTracker::new();
        let exhausted = vec![make_quota(100.0, 100.0)];
        let alert = tracker.update(&pid(ProviderKind::Claude), "Claude", &exhausted);
        assert!(alert.is_none(), "首次 Exhausted 数据不应触发告警");
    }

    #[test]
    fn test_low_to_normal_no_alert() {
        let mut tracker = QuotaAlertTracker::new();
        let normal = vec![make_quota(30.0, 100.0)];
        tracker.update(&pid(ProviderKind::Claude), "Claude", &normal);

        let low = vec![make_quota(92.0, 100.0)];
        tracker.update(&pid(ProviderKind::Claude), "Claude", &low);

        let back_normal = vec![make_quota(30.0, 100.0)];
        let alert = tracker.update(&pid(ProviderKind::Claude), "Claude", &back_normal);
        assert!(alert.is_none(), "Low → Normal 不应触发通知");
    }

    #[test]
    fn test_full_cycle_alerts_re_fire() {
        let mut tracker = QuotaAlertTracker::new();
        let claude = pid(ProviderKind::Claude);
        let normal = vec![make_quota(30.0, 100.0)];
        tracker.update(&claude, "Claude", &normal);

        let low = vec![make_quota(92.0, 100.0)];
        assert!(matches!(
            tracker.update(&claude, "Claude", &low),
            Some(QuotaAlert::LowQuota { .. })
        ));

        let exhausted = vec![make_quota(100.0, 100.0)];
        assert!(matches!(
            tracker.update(&claude, "Claude", &exhausted),
            Some(QuotaAlert::Exhausted { .. })
        ));

        assert!(matches!(
            tracker.update(&claude, "Claude", &normal),
            Some(QuotaAlert::Recovered { .. })
        ));

        assert!(
            matches!(
                tracker.update(&claude, "Claude", &low),
                Some(QuotaAlert::LowQuota { .. })
            ),
            "恢复后重新进入 Low 应该再次通知"
        );
    }

    /// balance_only 配额 (limit=0, used=0) 不应被视为 0% 剩余并触发误报。
    /// 回归测试：曾误用 percent_remaining() 导致 balance_only 永远 = 0% → 误报 Exhausted。
    #[test]
    fn balance_only_quotas_do_not_trigger_false_alerts() {
        use crate::models::QuotaType;

        let mut tracker = QuotaAlertTracker::new();
        let provider = pid(ProviderKind::Claude);

        // balance_only: limit=0, remaining_balance=5.0
        let balance_only = vec![QuotaInfo::balance_only(
            "credits",
            5.0,
            None,
            QuotaType::Credit,
            None,
        )];
        tracker.update(&provider, "Claude", &balance_only);

        // 第二次更新不应触发告警（balance_only 的 limit=0，remaining 应被视为 100%）
        let alert = tracker.update(&provider, "Claude", &balance_only);
        assert!(
            alert.is_none(),
            "balance_only quotas should not trigger alerts"
        );
    }
}
