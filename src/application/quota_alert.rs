//! Quota alert domain types and transition tracking.

use crate::models::{ProviderId, QuotaInfo};
use log::info;
use std::collections::HashMap;

/// Provider 配额的告警状态
///
/// 阈值有意与托盘图标状态阈值错开（`QuotaInfo::status_level`：剩余 >50% Green、
/// 20~50% Yellow、<20% Red）：图标是"瞟一眼"的早预警，通知是打断用户的晚警报——
/// 图标变红（<20%）时先不打扰，剩余 ≤10% 才发第一条 Low 通知，耗尽才发 Exhausted。
/// 若产品决策要求对齐两套阈值，调整 `AlertState::from_remaining` 即可。
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
    /// 余量已耗尽
    Exhausted { provider_name: String },
    /// 配额已恢复（从耗尽状态）
    Recovered {
        provider_name: String,
        remaining_pct: f64,
    },
    UsageProgress {
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

struct ProviderTrackState {
    alert_state: AlertState,
    previous_remaining: Option<f64>,
    usage_baseline_remaining: Option<f64>,
    usage_step_pct: u8,
}

const USAGE_EPSILON: f64 = 1e-9;

/// 追踪每个 Provider 的配额告警状态，检测状态转换并产生告警事件。
///
/// 设计为应用层纯业务组件：只输出“应该发什么通知”，不关心具体 OS 发送方式。
#[derive(Default)]
pub struct QuotaAlertTracker {
    states: HashMap<ProviderId, ProviderTrackState>,
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
        usage_step_pct: u8,
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

        let usage_remaining = quotas
            .iter()
            .filter(|q| q.limit > 0.0)
            .map(|q| (100.0 - q.percentage()).max(0.0))
            .reduce(f64::min);

        let new_state = AlertState::from_remaining(worst_remaining);
        let usage_step_pct = usage_step_pct.min(100);

        // 首次数据只建立基线，不触发告警（避免启动时误报）
        let Some(entry) = self.states.get_mut(id) else {
            self.states.insert(
                id.clone(),
                ProviderTrackState {
                    alert_state: new_state,
                    previous_remaining: usage_remaining,
                    usage_baseline_remaining: if usage_step_pct > 0 {
                        usage_remaining
                    } else {
                        None
                    },
                    usage_step_pct,
                },
            );
            return None;
        };

        let old_state = entry.alert_state;
        // 更新状态
        entry.alert_state = new_state;

        let name = provider_name.to_string();
        let legacy_alert = if old_state == new_state {
            // 状态未变化，不触发
            None
        } else {
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
                        provider_name: name.clone(),
                        remaining_pct: worst_remaining,
                    })
                }
                // 进入 Exhausted 状态
                (_, AlertState::Exhausted) => {
                    info!(target: "notification", "{} quota exhausted", name);
                    Some(QuotaAlert::Exhausted {
                        provider_name: name.clone(),
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
                        provider_name: name.clone(),
                        remaining_pct: worst_remaining,
                    })
                }
                // 其他转换不触发通知
                _ => None,
            }
        };

        let usage_alert = if legacy_alert.is_some() {
            entry.usage_baseline_remaining = if usage_step_pct > 0 {
                usage_remaining
            } else {
                None
            };
            None
        } else {
            Self::usage_progress(entry, usage_remaining, usage_step_pct, provider_name)
        };

        entry.previous_remaining = usage_remaining;
        entry.usage_step_pct = usage_step_pct;

        legacy_alert.or(usage_alert)
    }

    fn usage_progress(
        entry: &mut ProviderTrackState,
        usage_remaining: Option<f64>,
        usage_step_pct: u8,
        provider_name: &str,
    ) -> Option<QuotaAlert> {
        let (Some(current), true) = (usage_remaining, usage_step_pct > 0) else {
            entry.usage_baseline_remaining = None;
            return None;
        };

        let rebounded = entry
            .previous_remaining
            .is_some_and(|prev| current > prev + USAGE_EPSILON);
        if entry.usage_step_pct != usage_step_pct || rebounded {
            entry.usage_baseline_remaining = Some(current);
            return None;
        }

        let Some(baseline) = entry.usage_baseline_remaining else {
            entry.usage_baseline_remaining = Some(current);
            return None;
        };

        if baseline - current >= f64::from(usage_step_pct) - USAGE_EPSILON {
            entry.usage_baseline_remaining = Some(current);
            info!(
                target: "notification",
                "{} usage step reached: {:.1}% remaining",
                provider_name,
                current
            );
            Some(QuotaAlert::UsageProgress {
                provider_name: provider_name.to_string(),
                remaining_pct: current,
            })
        } else {
            None
        }
    }

    pub fn reset_usage(&mut self, id: &ProviderId) {
        if let Some(entry) = self.states.get_mut(id) {
            entry.usage_baseline_remaining = None;
        }
    }

    pub fn reset_all_usage(&mut self) {
        for entry in self.states.values_mut() {
            entry.usage_baseline_remaining = None;
        }
    }

    pub fn remove(&mut self, id: &ProviderId) {
        self.states.remove(id);
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

    fn remaining(remaining_pct: f64) -> Vec<QuotaInfo> {
        vec![make_quota(100.0 - remaining_pct, 100.0)]
    }

    fn assert_usage_progress(alert: Option<QuotaAlert>, expected_remaining: f64) {
        assert_usage_progress_for(alert, "Claude", expected_remaining);
    }

    fn assert_usage_progress_for(
        alert: Option<QuotaAlert>,
        expected_name: &str,
        expected_remaining: f64,
    ) {
        match alert {
            Some(QuotaAlert::UsageProgress {
                provider_name,
                remaining_pct,
            }) => {
                assert_eq!(provider_name, expected_name);
                assert!(
                    (remaining_pct - expected_remaining).abs() < 1e-9,
                    "expected remaining {expected_remaining}, got {remaining_pct}"
                );
            }
            other => panic!("expected UsageProgress({expected_remaining}), got {other:?}"),
        }
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
        let alert = tracker.update(&pid(ProviderKind::Claude), "Claude", &quotas, 0);
        assert!(alert.is_none(), "首次正常数据不应触发告警");
    }

    #[test]
    fn test_normal_to_low() {
        let mut tracker = QuotaAlertTracker::new();
        let normal = vec![make_quota(30.0, 100.0)];
        tracker.update(&pid(ProviderKind::Claude), "Claude", &normal, 0);

        let low = vec![make_quota(92.0, 100.0)];
        let alert = tracker.update(&pid(ProviderKind::Claude), "Claude", &low, 0);
        assert!(matches!(alert, Some(QuotaAlert::LowQuota { .. })));
    }

    #[test]
    fn test_low_to_exhausted() {
        let mut tracker = QuotaAlertTracker::new();
        let normal = vec![make_quota(30.0, 100.0)];
        tracker.update(&pid(ProviderKind::Claude), "Claude", &normal, 0);

        let low = vec![make_quota(95.0, 100.0)];
        tracker.update(&pid(ProviderKind::Claude), "Claude", &low, 0);

        let exhausted = vec![make_quota(100.0, 100.0)];
        let alert = tracker.update(&pid(ProviderKind::Claude), "Claude", &exhausted, 0);
        assert!(matches!(alert, Some(QuotaAlert::Exhausted { .. })));
    }

    #[test]
    fn test_normal_to_exhausted_directly() {
        let mut tracker = QuotaAlertTracker::new();
        let normal = vec![make_quota(30.0, 100.0)];
        tracker.update(&pid(ProviderKind::Claude), "Claude", &normal, 0);

        let exhausted = vec![make_quota(100.0, 100.0)];
        let alert = tracker.update(&pid(ProviderKind::Claude), "Claude", &exhausted, 0);
        assert!(matches!(alert, Some(QuotaAlert::Exhausted { .. })));
    }

    #[test]
    fn test_exhausted_to_recovery() {
        let mut tracker = QuotaAlertTracker::new();
        let normal = vec![make_quota(30.0, 100.0)];
        tracker.update(&pid(ProviderKind::Claude), "Claude", &normal, 0);

        let exhausted = vec![make_quota(100.0, 100.0)];
        tracker.update(&pid(ProviderKind::Claude), "Claude", &exhausted, 0);

        let recovered = vec![make_quota(50.0, 100.0)];
        let alert = tracker.update(&pid(ProviderKind::Claude), "Claude", &recovered, 0);
        assert!(matches!(alert, Some(QuotaAlert::Recovered { .. })));
    }

    #[test]
    fn test_exhausted_to_low_still_recovers() {
        let mut tracker = QuotaAlertTracker::new();
        let normal = vec![make_quota(30.0, 100.0)];
        tracker.update(&pid(ProviderKind::Claude), "Claude", &normal, 0);

        let exhausted = vec![make_quota(100.0, 100.0)];
        tracker.update(&pid(ProviderKind::Claude), "Claude", &exhausted, 0);

        let low = vec![make_quota(95.0, 100.0)];
        let alert = tracker.update(&pid(ProviderKind::Claude), "Claude", &low, 0);
        assert!(
            matches!(alert, Some(QuotaAlert::Recovered { .. })),
            "从耗尽恢复到 Low 也应触发恢复通知"
        );
    }

    #[test]
    fn test_repeated_state_no_alert() {
        let mut tracker = QuotaAlertTracker::new();
        let normal = vec![make_quota(30.0, 100.0)];
        tracker.update(&pid(ProviderKind::Claude), "Claude", &normal, 0);

        let low = vec![make_quota(92.0, 100.0)];
        tracker.update(&pid(ProviderKind::Claude), "Claude", &low, 0);

        let still_low = vec![make_quota(93.0, 100.0)];
        let alert = tracker.update(&pid(ProviderKind::Claude), "Claude", &still_low, 0);
        assert!(alert.is_none(), "重复 Low 状态不应重复告警");
    }

    #[test]
    fn test_worst_quota_determines_state() {
        let mut tracker = QuotaAlertTracker::new();
        let normal = vec![make_quota(30.0, 100.0)];
        tracker.update(&pid(ProviderKind::Claude), "Claude", &normal, 0);

        let mixed = vec![make_quota(30.0, 100.0), make_quota(95.0, 100.0)];
        let alert = tracker.update(&pid(ProviderKind::Claude), "Claude", &mixed, 0);
        assert!(
            matches!(alert, Some(QuotaAlert::LowQuota { .. })),
            "应取最差的 quota 决定状态"
        );
    }

    #[test]
    fn test_empty_quotas_no_alert() {
        let mut tracker = QuotaAlertTracker::new();
        let alert = tracker.update(&pid(ProviderKind::Claude), "Claude", &[], 0);
        assert!(alert.is_none(), "空 quotas 不应触发告警");
    }

    #[test]
    fn test_independent_providers() {
        let mut tracker = QuotaAlertTracker::new();

        let normal = vec![make_quota(30.0, 100.0)];
        tracker.update(&pid(ProviderKind::Claude), "Claude", &normal, 0);
        tracker.update(&pid(ProviderKind::Gemini), "Gemini", &normal, 0);

        let low = vec![make_quota(92.0, 100.0)];
        let alert = tracker.update(&pid(ProviderKind::Claude), "Claude", &low, 0);
        assert!(matches!(alert, Some(QuotaAlert::LowQuota { .. })));

        let still_normal = vec![make_quota(40.0, 100.0)];
        let alert = tracker.update(&pid(ProviderKind::Gemini), "Gemini", &still_normal, 0);
        assert!(alert.is_none(), "Gemini 状态未变，不应触发");
    }

    #[test]
    fn test_first_data_low_no_alert() {
        let mut tracker = QuotaAlertTracker::new();
        let low = vec![make_quota(95.0, 100.0)];
        let alert = tracker.update(&pid(ProviderKind::Claude), "Claude", &low, 0);
        assert!(alert.is_none(), "首次 Low 数据不应触发告警");
    }

    #[test]
    fn test_first_data_exhausted_no_alert() {
        let mut tracker = QuotaAlertTracker::new();
        let exhausted = vec![make_quota(100.0, 100.0)];
        let alert = tracker.update(&pid(ProviderKind::Claude), "Claude", &exhausted, 0);
        assert!(alert.is_none(), "首次 Exhausted 数据不应触发告警");
    }

    #[test]
    fn test_low_to_normal_no_alert() {
        let mut tracker = QuotaAlertTracker::new();
        let normal = vec![make_quota(30.0, 100.0)];
        tracker.update(&pid(ProviderKind::Claude), "Claude", &normal, 0);

        let low = vec![make_quota(92.0, 100.0)];
        tracker.update(&pid(ProviderKind::Claude), "Claude", &low, 0);

        let back_normal = vec![make_quota(30.0, 100.0)];
        let alert = tracker.update(&pid(ProviderKind::Claude), "Claude", &back_normal, 0);
        assert!(alert.is_none(), "Low → Normal 不应触发通知");
    }

    #[test]
    fn test_full_cycle_alerts_re_fire() {
        let mut tracker = QuotaAlertTracker::new();
        let claude = pid(ProviderKind::Claude);
        let normal = vec![make_quota(30.0, 100.0)];
        tracker.update(&claude, "Claude", &normal, 0);

        let low = vec![make_quota(92.0, 100.0)];
        assert!(matches!(
            tracker.update(&claude, "Claude", &low, 0),
            Some(QuotaAlert::LowQuota { .. })
        ));

        let exhausted = vec![make_quota(100.0, 100.0)];
        assert!(matches!(
            tracker.update(&claude, "Claude", &exhausted, 0),
            Some(QuotaAlert::Exhausted { .. })
        ));

        assert!(matches!(
            tracker.update(&claude, "Claude", &normal, 0),
            Some(QuotaAlert::Recovered { .. })
        ));

        assert!(
            matches!(
                tracker.update(&claude, "Claude", &low, 0),
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
        tracker.update(&provider, "Claude", &balance_only, 0);

        // 第二次更新不应触发告警（balance_only 的 limit=0，remaining 应被视为 100%）
        let alert = tracker.update(&provider, "Claude", &balance_only, 0);
        assert!(
            alert.is_none(),
            "balance_only quotas should not trigger alerts"
        );
    }

    #[test]
    fn test_usage_step_fires_at_step_boundaries() {
        let mut tracker = QuotaAlertTracker::new();
        let claude = pid(ProviderKind::Claude);

        assert!(tracker
            .update(&claude, "Claude", &remaining(83.0), 5)
            .is_none());
        assert!(tracker
            .update(&claude, "Claude", &remaining(81.0), 5)
            .is_none());
        assert!(tracker
            .update(&claude, "Claude", &remaining(79.0), 5)
            .is_none());
        assert_usage_progress(tracker.update(&claude, "Claude", &remaining(78.0), 5), 78.0);
        assert_usage_progress(tracker.update(&claude, "Claude", &remaining(73.0), 5), 73.0);
    }

    #[test]
    fn test_usage_step_large_drop_fires_once() {
        let mut tracker = QuotaAlertTracker::new();
        let claude = pid(ProviderKind::Claude);

        tracker.update(&claude, "Claude", &remaining(73.0), 5);

        assert_usage_progress(tracker.update(&claude, "Claude", &remaining(60.0), 5), 60.0);
        assert!(tracker
            .update(&claude, "Claude", &remaining(59.0), 5)
            .is_none());
        assert_usage_progress(tracker.update(&claude, "Claude", &remaining(55.0), 5), 55.0);
    }

    #[test]
    fn test_usage_step_rebuilds_baseline_on_rebound() {
        let mut tracker = QuotaAlertTracker::new();
        let claude = pid(ProviderKind::Claude);

        tracker.update(&claude, "Claude", &remaining(83.0), 5);
        tracker.update(&claude, "Claude", &remaining(81.0), 5);
        assert!(tracker
            .update(&claude, "Claude", &remaining(82.0), 5)
            .is_none());
        assert_usage_progress(tracker.update(&claude, "Claude", &remaining(77.0), 5), 77.0);
    }

    #[test]
    fn test_usage_step_yields_to_legacy_alerts() {
        let mut tracker = QuotaAlertTracker::new();
        let claude = pid(ProviderKind::Claude);

        tracker.update(&claude, "Claude", &remaining(13.0), 5);
        assert!(matches!(
            tracker.update(&claude, "Claude", &remaining(8.0), 5),
            Some(QuotaAlert::LowQuota { remaining_pct, .. }) if (remaining_pct - 8.0).abs() < 1e-9
        ));
        assert!(tracker
            .update(&claude, "Claude", &remaining(8.0), 5)
            .is_none());
        assert_usage_progress(tracker.update(&claude, "Claude", &remaining(3.0), 5), 3.0);
        assert!(matches!(
            tracker.update(&claude, "Claude", &remaining(0.0), 5),
            Some(QuotaAlert::Exhausted { .. })
        ));
        assert!(matches!(
            tracker.update(&claude, "Claude", &remaining(50.0), 5),
            Some(QuotaAlert::Recovered { .. })
        ));
        assert_usage_progress(tracker.update(&claude, "Claude", &remaining(45.0), 5), 45.0);
    }

    #[test]
    fn test_usage_step_change_rebuilds_baseline() {
        let mut tracker = QuotaAlertTracker::new();
        let claude = pid(ProviderKind::Claude);

        tracker.update(&claude, "Claude", &remaining(100.0), 5);
        tracker.update(&claude, "Claude", &remaining(97.0), 5);

        assert!(tracker
            .update(&claude, "Claude", &remaining(95.0), 10)
            .is_none());
        assert!(tracker
            .update(&claude, "Claude", &remaining(90.0), 10)
            .is_none());
        assert_usage_progress(
            tracker.update(&claude, "Claude", &remaining(85.0), 10),
            85.0,
        );
    }

    #[test]
    fn test_usage_step_zero_disables_progress() {
        let mut tracker = QuotaAlertTracker::new();
        let claude = pid(ProviderKind::Claude);

        tracker.update(&claude, "Claude", &remaining(83.0), 0);
        assert!(tracker
            .update(&claude, "Claude", &remaining(78.0), 0)
            .is_none());
        assert!(tracker
            .update(&claude, "Claude", &remaining(50.0), 0)
            .is_none());
    }

    #[test]
    fn test_usage_step_empty_quotas_not_a_sample() {
        let mut tracker = QuotaAlertTracker::new();
        let claude = pid(ProviderKind::Claude);

        tracker.update(&claude, "Claude", &remaining(83.0), 5);
        assert!(tracker.update(&claude, "Claude", &[], 5).is_none());
        assert_usage_progress(tracker.update(&claude, "Claude", &remaining(78.0), 5), 78.0);
    }

    #[test]
    fn test_usage_step_balance_only_never_alerts() {
        use crate::models::QuotaType;

        let mut tracker = QuotaAlertTracker::new();
        let provider = pid(ProviderKind::Claude);

        let balance = vec![QuotaInfo::balance_only(
            "credits",
            5.0,
            None,
            QuotaType::Credit,
            None,
        )];
        let smaller_balance = vec![QuotaInfo::balance_only(
            "credits",
            1.0,
            None,
            QuotaType::Credit,
            None,
        )];

        assert!(tracker.update(&provider, "Claude", &balance, 5).is_none());
        assert!(tracker
            .update(&provider, "Claude", &smaller_balance, 5)
            .is_none());
    }

    #[test]
    fn test_usage_step_uses_worst_quota() {
        let mut tracker = QuotaAlertTracker::new();
        let claude = pid(ProviderKind::Claude);

        let first = vec![
            QuotaInfo::new("a", 20.0, 100.0),
            QuotaInfo::new("b", 70.0, 100.0),
        ];
        let second = vec![
            QuotaInfo::new("a", 25.0, 100.0),
            QuotaInfo::new("b", 76.0, 100.0),
        ];

        assert!(tracker.update(&claude, "Claude", &first, 5).is_none());
        assert_usage_progress(tracker.update(&claude, "Claude", &second, 5), 24.0);
    }

    #[test]
    fn test_usage_step_independent_providers() {
        let mut tracker = QuotaAlertTracker::new();
        let claude = pid(ProviderKind::Claude);
        let gemini = pid(ProviderKind::Gemini);

        tracker.update(&claude, "Claude", &remaining(83.0), 5);
        tracker.update(&gemini, "Gemini", &remaining(90.0), 5);

        assert_usage_progress(tracker.update(&claude, "Claude", &remaining(78.0), 5), 78.0);
        assert!(tracker
            .update(&gemini, "Gemini", &remaining(86.0), 5)
            .is_none());
        assert_usage_progress_for(
            tracker.update(&gemini, "Gemini", &remaining(85.0), 5),
            "Gemini",
            85.0,
        );
    }

    #[test]
    fn test_reset_usage_clears_baseline_but_keeps_alert_state() {
        let mut tracker = QuotaAlertTracker::new();
        let claude = pid(ProviderKind::Claude);

        tracker.update(&claude, "Claude", &remaining(83.0), 5);
        tracker.reset_usage(&claude);
        assert!(tracker
            .update(&claude, "Claude", &remaining(78.0), 5)
            .is_none());
        assert_usage_progress(tracker.update(&claude, "Claude", &remaining(73.0), 5), 73.0);

        assert!(matches!(
            tracker.update(&claude, "Claude", &remaining(8.0), 5),
            Some(QuotaAlert::LowQuota { .. })
        ));
        tracker.reset_usage(&claude);
        assert!(matches!(
            tracker.update(&claude, "Claude", &remaining(0.0), 5),
            Some(QuotaAlert::Exhausted { .. })
        ));
        assert!(matches!(
            tracker.update(&claude, "Claude", &remaining(50.0), 5),
            Some(QuotaAlert::Recovered { .. })
        ));
    }

    #[test]
    fn test_reset_all_usage_clears_all_baselines() {
        let mut tracker = QuotaAlertTracker::new();
        let claude = pid(ProviderKind::Claude);
        let gemini = pid(ProviderKind::Gemini);

        tracker.update(&claude, "Claude", &remaining(83.0), 5);
        tracker.update(&gemini, "Gemini", &remaining(90.0), 5);
        tracker.reset_all_usage();

        assert!(tracker
            .update(&claude, "Claude", &remaining(78.0), 5)
            .is_none());
        assert!(tracker
            .update(&gemini, "Gemini", &remaining(85.0), 5)
            .is_none());

        assert!(matches!(
            tracker.update(&claude, "Claude", &remaining(8.0), 5),
            Some(QuotaAlert::LowQuota { .. })
        ));
        assert!(matches!(
            tracker.update(&gemini, "Gemini", &remaining(7.0), 5),
            Some(QuotaAlert::LowQuota { .. })
        ));
        tracker.reset_all_usage();
        assert!(matches!(
            tracker.update(&claude, "Claude", &remaining(0.0), 5),
            Some(QuotaAlert::Exhausted { .. })
        ));
        assert!(matches!(
            tracker.update(&gemini, "Gemini", &remaining(0.0), 5),
            Some(QuotaAlert::Exhausted { .. })
        ));
    }

    #[test]
    fn test_remove_clears_provider_state() {
        let mut tracker = QuotaAlertTracker::new();
        let claude = pid(ProviderKind::Claude);

        tracker.update(&claude, "Claude", &remaining(13.0), 5);
        assert!(matches!(
            tracker.update(&claude, "Claude", &remaining(8.0), 5),
            Some(QuotaAlert::LowQuota { .. })
        ));
        tracker.remove(&claude);

        assert!(tracker
            .update(&claude, "Claude", &remaining(8.0), 5)
            .is_none());
    }
}
