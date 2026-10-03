//! Quota alert domain types and transition tracking.

use crate::models::{ProviderId, QuotaInfo, QuotaRules};
use log::info;
use std::collections::HashMap;

/// Provider 配额的告警状态
///
/// 阈值档位按各 quota 对应单位（百分比 / 货币 / 积分·原生额度）的 `notify`
/// 阈值判定剩余值，与托盘图标使用的 `warning` / `critical` 颜色阈值错开：
/// 图标是"瞟一眼"的早预警，通知是打断用户的晚警报。两组阈值都来自同一份
/// `QuotaRules` 配置。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AlertState {
    /// 剩余值高于该单位 notify 阈值
    Normal,
    /// 剩余值 ≤ notify 阈值且 > 0
    Low,
    /// 剩余值 ≤ 0（耗尽或超用）
    Exhausted,
}

impl AlertState {
    fn severity(self) -> u8 {
        match self {
            Self::Normal => 0,
            Self::Low => 1,
            Self::Exhausted => 2,
        }
    }
}

/// 应该发送的额度通知领域事件。
///
/// 事件只描述业务结果，不携带通知渠道、声音或本地化文案。
#[derive(Debug, Clone, PartialEq)]
pub enum QuotaNotificationEvent {
    /// 剩余值 ≤ 该单位 notify 阈值。`quota` 是触发告警的具体额度项。
    LowQuota {
        provider_name: String,
        quota: QuotaInfo,
    },
    /// 剩余值 ≤ 0（耗尽或超用）。
    Exhausted {
        provider_name: String,
        quota: QuotaInfo,
    },
    /// 配额已恢复（从耗尽状态，包含恢复到 Low 档）。
    Recovered {
        provider_name: String,
        quota: QuotaInfo,
    },
    UsageProgress {
        provider_name: String,
        remaining_pct: f64,
    },
}

#[derive(Debug, Clone)]
struct ThresholdAlertState {
    alert_state: AlertState,
    quota_key: String,
}

#[derive(Debug, Clone, Copy)]
struct UsageProgressState {
    previous_remaining: Option<f64>,
    usage_baseline_remaining: Option<f64>,
    usage_step_pct: u8,
}

#[derive(Debug, Clone)]
struct ProviderAlertState {
    threshold: ThresholdAlertState,
    usage: UsageProgressState,
}

/// 一次刷新成功后交给告警引擎的统一领域输入。
///
/// 状态阈值策略和用量步长策略必须消费同一份快照与 effective rules，
/// 这样配置变化不会让两类提醒看到不同版本的额度数据。
pub struct QuotaObservation<'a> {
    pub provider_id: &'a ProviderId,
    pub provider_name: &'a str,
    pub quotas: &'a [QuotaInfo],
    pub rules: &'a QuotaRules,
    pub usage_step_pct: u8,
}

const USAGE_EPSILON: f64 = 1e-9;

/// 追踪每个 Provider 的两类额度提醒状态，并产出领域事件。
///
/// 设计为应用层纯业务组件：只输出“应该发什么通知”，不关心具体 OS 发送方式。
#[derive(Default)]
pub struct AlertEngine {
    states: HashMap<ProviderId, ProviderAlertState>,
}

struct Observation<'a> {
    alert_state: AlertState,
    quota: &'a QuotaInfo,
    remaining: f64,
    usage_remaining: Option<f64>,
}

impl AlertEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn rebaseline_alerts(&mut self, id: &ProviderId, quotas: &[QuotaInfo], rules: &QuotaRules) {
        let Some(obs) = observe(quotas, rules) else {
            return;
        };
        if let Some(entry) = self.states.get_mut(id) {
            entry.threshold.alert_state = obs.alert_state;
            entry.threshold.quota_key = obs.quota.stable_key.clone();
        }
    }

    /// 根据统一额度快照评估所有提醒策略，返回领域事件。
    ///
    /// 阈值档位对每个有效 measurement 按对应单位的 `notify` 阈值判定，
    /// 取严重等级最高的 quota（同档保留输入顺序的第一个）；货币 / 积分
    /// 用原生剩余值，不换算成百分比。无有效 measurement 时不推进任何基线。
    /// 步长告警仍以有总额 quota 的最差剩余百分比采样。
    pub fn evaluate(&mut self, observation: QuotaObservation<'_>) -> Vec<QuotaNotificationEvent> {
        let QuotaObservation {
            provider_id: id,
            provider_name,
            quotas,
            rules,
            usage_step_pct,
        } = observation;
        let Some(obs) = observe(quotas, rules) else {
            return Vec::new();
        };
        let new_state = obs.alert_state;
        let usage_remaining = obs.usage_remaining;
        let usage_step_pct = usage_step_pct.min(100);

        // 首次数据只建立基线，不触发告警（避免启动时误报）
        let Some(entry) = self.states.get_mut(id) else {
            self.states.insert(
                id.clone(),
                ProviderAlertState {
                    threshold: ThresholdAlertState {
                        alert_state: new_state,
                        quota_key: obs.quota.stable_key.clone(),
                    },
                    usage: UsageProgressState {
                        previous_remaining: usage_remaining,
                        usage_baseline_remaining: if usage_step_pct > 0 {
                            usage_remaining
                        } else {
                            None
                        },
                        usage_step_pct,
                    },
                },
            );
            return Vec::new();
        };

        let threshold_event =
            ThresholdAlertPolicy::evaluate(&mut entry.threshold, &obs, quotas, provider_name);
        let usage_event = if threshold_event.is_some() {
            // 同一刷新内状态提醒优先，同时重建用量基线，避免下一次刷新重复报告同一突变。
            UsageProgressPolicy::reset_after_status(
                &mut entry.usage,
                usage_step_pct,
                usage_remaining,
            );
            None
        } else {
            UsageProgressPolicy::evaluate(
                &mut entry.usage,
                usage_remaining,
                usage_step_pct,
                provider_name,
            )
        };

        entry.usage.previous_remaining = usage_remaining;
        entry.usage.usage_step_pct = usage_step_pct;
        entry.threshold.quota_key = obs.quota.stable_key.clone();

        threshold_event.into_iter().chain(usage_event).collect()
    }

    /// 兼容旧调用方的单事件入口；新代码应使用 `evaluate`。
    pub fn update(
        &mut self,
        id: &ProviderId,
        provider_name: &str,
        quotas: &[QuotaInfo],
        rules: &QuotaRules,
        usage_step_pct: u8,
    ) -> Option<QuotaNotificationEvent> {
        self.evaluate(QuotaObservation {
            provider_id: id,
            provider_name,
            quotas,
            rules,
            usage_step_pct,
        })
        .into_iter()
        .next()
    }

    pub fn reset_usage(&mut self, id: &ProviderId) {
        if let Some(entry) = self.states.get_mut(id) {
            entry.usage.usage_baseline_remaining = None;
        }
    }

    pub fn reset_all_usage(&mut self) {
        for entry in self.states.values_mut() {
            entry.usage.usage_baseline_remaining = None;
        }
    }

    pub fn remove(&mut self, id: &ProviderId) {
        self.states.remove(id);
    }
}

/// 状态阈值策略：只负责 Normal / Low / Exhausted / Recovered 状态迁移。
///
/// 状态机按 provider 维度只记单档 worst 状态，因此同一刷新内的反向变迁会被合并：
/// 若 quota A 从 Exhausted 恢复、quota B 恰好首次跌入 Low，只会发出 Recovered(A)，
/// B 的 LowQuota 不会补发（状态已记为 Low，后续同档去重），直到 B 耗尽或先回升
/// 再跌回。按 quota_key 维度跟踪可消除该窗口，但一次快照可能产出多条通知，
/// 单档合并是有意的降噪取舍。
struct ThresholdAlertPolicy;

impl ThresholdAlertPolicy {
    fn evaluate(
        state: &mut ThresholdAlertState,
        obs: &Observation<'_>,
        quotas: &[QuotaInfo],
        provider_name: &str,
    ) -> Option<QuotaNotificationEvent> {
        let old_state = state.alert_state;
        let old_quota_key = state.quota_key.clone();
        state.alert_state = obs.alert_state;
        let name = provider_name.to_string();

        match (old_state, obs.alert_state) {
            (old, new) if old == new => None,
            (AlertState::Normal, AlertState::Low) => {
                info!(
                    target: "notification",
                    "{} quota low: {:.1} remaining",
                    name,
                    obs.remaining
                );
                Some(QuotaNotificationEvent::LowQuota {
                    provider_name: name,
                    quota: obs.quota.clone(),
                })
            }
            (_, AlertState::Exhausted) => {
                info!(target: "notification", "{} quota exhausted", name);
                Some(QuotaNotificationEvent::Exhausted {
                    provider_name: name,
                    quota: obs.quota.clone(),
                })
            }
            (AlertState::Exhausted, _) => {
                let recovered = quotas
                    .iter()
                    .find(|q| q.stable_key == old_quota_key && q.threshold_measurement().is_some())
                    .unwrap_or(obs.quota);
                let remaining = recovered
                    .threshold_measurement()
                    .map(|m| m.remaining)
                    .unwrap_or(obs.remaining);
                info!(
                    target: "notification",
                    "{} quota recovered: {:.1} remaining",
                    name,
                    remaining
                );
                Some(QuotaNotificationEvent::Recovered {
                    provider_name: name,
                    quota: recovered.clone(),
                })
            }
            _ => None,
        }
    }
}

/// 用量变化策略：只负责按百分比步长维护累计下降基线。
struct UsageProgressPolicy;

impl UsageProgressPolicy {
    fn reset_after_status(
        state: &mut UsageProgressState,
        usage_step_pct: u8,
        usage_remaining: Option<f64>,
    ) {
        state.usage_baseline_remaining = if usage_step_pct > 0 {
            usage_remaining
        } else {
            None
        };
    }

    fn evaluate(
        state: &mut UsageProgressState,
        usage_remaining: Option<f64>,
        usage_step_pct: u8,
        provider_name: &str,
    ) -> Option<QuotaNotificationEvent> {
        let (Some(current), true) = (usage_remaining, usage_step_pct > 0) else {
            state.usage_baseline_remaining = None;
            return None;
        };

        let rebounded = state
            .previous_remaining
            .is_some_and(|prev| current > prev + USAGE_EPSILON);
        if state.usage_step_pct != usage_step_pct || rebounded {
            state.usage_baseline_remaining = Some(current);
            return None;
        }

        let Some(baseline) = state.usage_baseline_remaining else {
            state.usage_baseline_remaining = Some(current);
            return None;
        };

        if baseline - current >= f64::from(usage_step_pct) - USAGE_EPSILON {
            state.usage_baseline_remaining = Some(current);
            info!(
                target: "notification",
                "{} usage step reached: {:.1}% remaining",
                provider_name,
                current
            );
            Some(QuotaNotificationEvent::UsageProgress {
                provider_name: provider_name.to_string(),
                remaining_pct: current,
            })
        } else {
            None
        }
    }
}

/// 从一组 quota 中选出阈值档位最坏的观测。
///
/// 逐 quota 计算有效 measurement 并按各自单位的 `notify` 阈值映射为
/// Normal / Low / Exhausted，取严重等级最高者（同档保留第一个，保证确定性）。
/// `usage_remaining` 独立采样所有 `limit > 0` quota 的最差剩余百分比
/// （步长是百分比语义；纯余额 quota 不参与，也不阻止阈值观测）。
/// 全部 quota 无有效 measurement 时返回 `None`。
fn observe<'a>(quotas: &'a [QuotaInfo], rules: &QuotaRules) -> Option<Observation<'a>> {
    let mut worst: Option<(AlertState, &'a QuotaInfo, f64)> = None;
    for quota in quotas {
        let Some(measurement) = quota.threshold_measurement() else {
            continue;
        };
        let state = if measurement.remaining <= 0.0 {
            AlertState::Exhausted
        } else if rules
            .thresholds(measurement.unit)
            .notify_threshold_reached(measurement)
        {
            AlertState::Low
        } else {
            AlertState::Normal
        };
        if worst
            .as_ref()
            .is_none_or(|(prev, _, _)| state.severity() > prev.severity())
        {
            worst = Some((state, quota, measurement.remaining));
        }
    }
    let (alert_state, quota, remaining) = worst?;

    let usage_remaining = quotas
        .iter()
        .filter(|q| !q.is_balance_only() && q.limit > 0.0 && q.threshold_measurement().is_some())
        .filter_map(|q| {
            let remaining = q.percent_remaining();
            remaining.is_finite().then_some(remaining.max(0.0))
        })
        .reduce(f64::min);

    Some(Observation {
        alert_state,
        quota,
        remaining,
        usage_remaining,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ProviderKind, QuotaThresholdUnit, QuotaThresholds, QuotaType};

    fn pid(kind: ProviderKind) -> ProviderId {
        ProviderId::BuiltIn(kind)
    }

    fn rules() -> QuotaRules {
        QuotaRules::default()
    }

    fn make_quota(used: f64, limit: f64) -> QuotaInfo {
        QuotaInfo::new("test", used, limit)
    }

    fn remaining(remaining_pct: f64) -> Vec<QuotaInfo> {
        vec![make_quota(100.0 - remaining_pct, 100.0)]
    }

    fn balance_quota(amount: f64) -> Vec<QuotaInfo> {
        vec![QuotaInfo::balance_only(
            "credits",
            amount,
            None,
            QuotaType::Credit,
            None,
        )]
    }

    fn assert_usage_progress(alert: Option<QuotaNotificationEvent>, expected_remaining: f64) {
        assert_usage_progress_for(alert, "Claude", expected_remaining);
    }

    fn assert_usage_progress_for(
        alert: Option<QuotaNotificationEvent>,
        expected_name: &str,
        expected_remaining: f64,
    ) {
        match alert {
            Some(QuotaNotificationEvent::UsageProgress {
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
    fn evaluate_returns_domain_events_and_keeps_status_priority() {
        let mut engine = AlertEngine::new();
        let provider = pid(ProviderKind::Claude);
        let normal = remaining(83.0);
        let low_after_large_drop = remaining(8.0);

        assert!(engine
            .evaluate(QuotaObservation {
                provider_id: &provider,
                provider_name: "Claude",
                quotas: &normal,
                rules: &rules(),
                usage_step_pct: 5,
            })
            .is_empty());

        let events = engine.evaluate(QuotaObservation {
            provider_id: &provider,
            provider_name: "Claude",
            quotas: &low_after_large_drop,
            rules: &rules(),
            usage_step_pct: 5,
        });
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], QuotaNotificationEvent::LowQuota { .. }));
    }

    #[test]
    fn test_no_alert_on_first_normal_data() {
        let mut tracker = AlertEngine::new();
        let quotas = vec![make_quota(30.0, 100.0)];
        let alert = tracker.update(&pid(ProviderKind::Claude), "Claude", &quotas, &rules(), 0);
        assert!(alert.is_none(), "首次正常数据不应触发告警");
    }

    #[test]
    fn test_normal_to_low() {
        let mut tracker = AlertEngine::new();
        let normal = vec![make_quota(30.0, 100.0)];
        tracker.update(&pid(ProviderKind::Claude), "Claude", &normal, &rules(), 0);

        let low = vec![make_quota(92.0, 100.0)];
        let alert = tracker.update(&pid(ProviderKind::Claude), "Claude", &low, &rules(), 0);
        assert!(matches!(
            alert,
            Some(QuotaNotificationEvent::LowQuota { .. })
        ));
    }

    #[test]
    fn test_low_to_exhausted() {
        let mut tracker = AlertEngine::new();
        let normal = vec![make_quota(30.0, 100.0)];
        tracker.update(&pid(ProviderKind::Claude), "Claude", &normal, &rules(), 0);

        let low = vec![make_quota(95.0, 100.0)];
        tracker.update(&pid(ProviderKind::Claude), "Claude", &low, &rules(), 0);

        let exhausted = vec![make_quota(100.0, 100.0)];
        let alert = tracker.update(
            &pid(ProviderKind::Claude),
            "Claude",
            &exhausted,
            &rules(),
            0,
        );
        assert!(matches!(
            alert,
            Some(QuotaNotificationEvent::Exhausted { .. })
        ));
    }

    #[test]
    fn test_normal_to_exhausted_directly() {
        let mut tracker = AlertEngine::new();
        let normal = vec![make_quota(30.0, 100.0)];
        tracker.update(&pid(ProviderKind::Claude), "Claude", &normal, &rules(), 0);

        let exhausted = vec![make_quota(100.0, 100.0)];
        let alert = tracker.update(
            &pid(ProviderKind::Claude),
            "Claude",
            &exhausted,
            &rules(),
            0,
        );
        assert!(matches!(
            alert,
            Some(QuotaNotificationEvent::Exhausted { .. })
        ));
    }

    #[test]
    fn test_exhausted_to_recovery() {
        let mut tracker = AlertEngine::new();
        let normal = vec![make_quota(30.0, 100.0)];
        tracker.update(&pid(ProviderKind::Claude), "Claude", &normal, &rules(), 0);

        let exhausted = vec![make_quota(100.0, 100.0)];
        tracker.update(
            &pid(ProviderKind::Claude),
            "Claude",
            &exhausted,
            &rules(),
            0,
        );

        let recovered = vec![make_quota(50.0, 100.0)];
        let alert = tracker.update(
            &pid(ProviderKind::Claude),
            "Claude",
            &recovered,
            &rules(),
            0,
        );
        assert!(matches!(
            alert,
            Some(QuotaNotificationEvent::Recovered { .. })
        ));
    }

    #[test]
    fn test_exhausted_to_low_still_recovers() {
        let mut tracker = AlertEngine::new();
        let normal = vec![make_quota(30.0, 100.0)];
        tracker.update(&pid(ProviderKind::Claude), "Claude", &normal, &rules(), 0);

        let exhausted = vec![make_quota(100.0, 100.0)];
        tracker.update(
            &pid(ProviderKind::Claude),
            "Claude",
            &exhausted,
            &rules(),
            0,
        );

        let low = vec![make_quota(95.0, 100.0)];
        let alert = tracker.update(&pid(ProviderKind::Claude), "Claude", &low, &rules(), 0);
        assert!(
            matches!(alert, Some(QuotaNotificationEvent::Recovered { .. })),
            "从耗尽恢复到 Low 也应触发恢复通知"
        );
    }

    #[test]
    fn test_repeated_state_no_alert() {
        let mut tracker = AlertEngine::new();
        let normal = vec![make_quota(30.0, 100.0)];
        tracker.update(&pid(ProviderKind::Claude), "Claude", &normal, &rules(), 0);

        let low = vec![make_quota(92.0, 100.0)];
        tracker.update(&pid(ProviderKind::Claude), "Claude", &low, &rules(), 0);

        let still_low = vec![make_quota(93.0, 100.0)];
        let alert = tracker.update(
            &pid(ProviderKind::Claude),
            "Claude",
            &still_low,
            &rules(),
            0,
        );
        assert!(alert.is_none(), "重复 Low 状态不应重复告警");
    }

    #[test]
    fn test_worst_quota_determines_state() {
        let mut tracker = AlertEngine::new();
        let normal = vec![make_quota(30.0, 100.0)];
        tracker.update(&pid(ProviderKind::Claude), "Claude", &normal, &rules(), 0);

        let mixed = vec![make_quota(30.0, 100.0), make_quota(95.0, 100.0)];
        let alert = tracker.update(&pid(ProviderKind::Claude), "Claude", &mixed, &rules(), 0);
        assert!(
            matches!(alert, Some(QuotaNotificationEvent::LowQuota { .. })),
            "应取最差的 quota 决定状态"
        );
    }

    #[test]
    fn test_empty_quotas_no_alert() {
        let mut tracker = AlertEngine::new();
        let alert = tracker.update(&pid(ProviderKind::Claude), "Claude", &[], &rules(), 0);
        assert!(alert.is_none(), "空 quotas 不应触发告警");
    }

    #[test]
    fn test_independent_providers() {
        let mut tracker = AlertEngine::new();

        let normal = vec![make_quota(30.0, 100.0)];
        tracker.update(&pid(ProviderKind::Claude), "Claude", &normal, &rules(), 0);
        tracker.update(&pid(ProviderKind::Gemini), "Gemini", &normal, &rules(), 0);

        let low = vec![make_quota(92.0, 100.0)];
        let alert = tracker.update(&pid(ProviderKind::Claude), "Claude", &low, &rules(), 0);
        assert!(matches!(
            alert,
            Some(QuotaNotificationEvent::LowQuota { .. })
        ));

        let still_normal = vec![make_quota(40.0, 100.0)];
        let alert = tracker.update(
            &pid(ProviderKind::Gemini),
            "Gemini",
            &still_normal,
            &rules(),
            0,
        );
        assert!(alert.is_none(), "Gemini 状态未变，不应触发");
    }

    #[test]
    fn test_first_data_low_no_alert() {
        let mut tracker = AlertEngine::new();
        let low = vec![make_quota(95.0, 100.0)];
        let alert = tracker.update(&pid(ProviderKind::Claude), "Claude", &low, &rules(), 0);
        assert!(alert.is_none(), "首次 Low 数据不应触发告警");
    }

    #[test]
    fn test_first_data_exhausted_no_alert() {
        let mut tracker = AlertEngine::new();
        let exhausted = vec![make_quota(100.0, 100.0)];
        let alert = tracker.update(
            &pid(ProviderKind::Claude),
            "Claude",
            &exhausted,
            &rules(),
            0,
        );
        assert!(alert.is_none(), "首次 Exhausted 数据不应触发告警");
    }

    #[test]
    fn test_low_to_normal_no_alert() {
        let mut tracker = AlertEngine::new();
        let normal = vec![make_quota(30.0, 100.0)];
        tracker.update(&pid(ProviderKind::Claude), "Claude", &normal, &rules(), 0);

        let low = vec![make_quota(92.0, 100.0)];
        tracker.update(&pid(ProviderKind::Claude), "Claude", &low, &rules(), 0);

        let back_normal = vec![make_quota(30.0, 100.0)];
        let alert = tracker.update(
            &pid(ProviderKind::Claude),
            "Claude",
            &back_normal,
            &rules(),
            0,
        );
        assert!(alert.is_none(), "Low → Normal 不应触发通知");
    }

    #[test]
    fn test_full_cycle_alerts_re_fire() {
        let mut tracker = AlertEngine::new();
        let claude = pid(ProviderKind::Claude);
        let normal = vec![make_quota(30.0, 100.0)];
        tracker.update(&claude, "Claude", &normal, &rules(), 0);

        let low = vec![make_quota(92.0, 100.0)];
        assert!(matches!(
            tracker.update(&claude, "Claude", &low, &rules(), 0),
            Some(QuotaNotificationEvent::LowQuota { .. })
        ));

        let exhausted = vec![make_quota(100.0, 100.0)];
        assert!(matches!(
            tracker.update(&claude, "Claude", &exhausted, &rules(), 0),
            Some(QuotaNotificationEvent::Exhausted { .. })
        ));

        assert!(matches!(
            tracker.update(&claude, "Claude", &normal, &rules(), 0),
            Some(QuotaNotificationEvent::Recovered { .. })
        ));

        assert!(
            matches!(
                tracker.update(&claude, "Claude", &low, &rules(), 0),
                Some(QuotaNotificationEvent::LowQuota { .. })
            ),
            "恢复后重新进入 Low 应该再次通知"
        );
    }

    /// balance_only 配额 (limit=0, used=0) 不应被视为 0% 剩余并触发误报。
    /// 回归测试：曾误用 percent_remaining() 导致 balance_only 永远 = 0% → 误报 Exhausted。
    #[test]
    fn balance_only_quotas_do_not_trigger_false_alerts() {
        let mut tracker = AlertEngine::new();
        let provider = pid(ProviderKind::Claude);

        // balance_only: limit=0, remaining_balance=5.0
        let balance_only = balance_quota(5.0);
        tracker.update(&provider, "Claude", &balance_only, &rules(), 0);

        // 第二次更新不应触发告警（balance_only 用原生余额判定，且不参与步长百分比采样）
        let alert = tracker.update(&provider, "Claude", &balance_only, &rules(), 0);
        assert!(alert.is_none(), "余额未跨档时不应触发告警");
    }

    #[test]
    fn balance_quota_uses_native_remaining_for_alerts() {
        let mut tracker = AlertEngine::new();
        let provider = pid(ProviderKind::Claude);

        assert!(tracker
            .update(&provider, "Claude", &balance_quota(5.0), &rules(), 0)
            .is_none());

        match tracker.update(&provider, "Claude", &balance_quota(0.5), &rules(), 0) {
            Some(QuotaNotificationEvent::LowQuota { quota, .. }) => {
                assert_eq!(quota.remaining_balance, Some(0.5));
            }
            other => panic!("expected LowQuota with balance quota, got {other:?}"),
        }

        assert!(matches!(
            tracker.update(&provider, "Claude", &balance_quota(0.0), &rules(), 0),
            Some(QuotaNotificationEvent::Exhausted { .. })
        ));

        assert!(matches!(
            tracker.update(&provider, "Claude", &balance_quota(5.0), &rules(), 0),
            Some(QuotaNotificationEvent::Recovered { .. })
        ));
    }

    #[test]
    fn recovered_reports_quota_that_actually_recovered() {
        let mut tracker = AlertEngine::new();
        let provider = pid(ProviderKind::Claude);
        let quotas = |balance: f64| {
            vec![
                make_quota(70.0, 100.0),
                QuotaInfo::balance_only("credits", balance, None, QuotaType::Credit, None),
            ]
        };

        tracker.update(&provider, "Claude", &quotas(5.0), &rules(), 0);
        assert!(matches!(
            tracker.update(&provider, "Claude", &quotas(0.0), &rules(), 0),
            Some(QuotaNotificationEvent::Exhausted { .. })
        ));

        match tracker.update(&provider, "Claude", &quotas(5.0), &rules(), 0) {
            Some(QuotaNotificationEvent::Recovered { quota, .. }) => {
                assert_eq!(quota.stable_key, "credits");
                assert_eq!(quota.remaining_balance, Some(5.0));
            }
            other => panic!("expected Recovered carrying the balance quota, got {other:?}"),
        }
    }

    #[test]
    fn mixed_units_alert_on_native_remaining() {
        let mut tracker = AlertEngine::new();
        let provider = pid(ProviderKind::Claude);

        let mixed = || {
            vec![
                make_quota(70.0, 100.0),
                QuotaInfo::balance_only("credits", 5.0, None, QuotaType::Credit, None),
            ]
        };
        assert!(tracker
            .update(&provider, "Claude", &mixed(), &rules(), 0)
            .is_none());

        let low_balance = vec![
            make_quota(70.0, 100.0),
            QuotaInfo::balance_only("credits", 0.5, None, QuotaType::Credit, None),
        ];
        match tracker.update(&provider, "Claude", &low_balance, &rules(), 0) {
            Some(QuotaNotificationEvent::LowQuota { quota, .. }) => {
                assert_eq!(quota.remaining_balance, Some(0.5));
            }
            other => panic!("expected LowQuota carrying the balance quota, got {other:?}"),
        }
    }

    #[test]
    fn credit_with_limit_100_uses_currency_unit() {
        let mut tracker = AlertEngine::new();
        let provider = pid(ProviderKind::Claude);

        let credits = vec![QuotaInfo::with_details(
            "credits",
            98.0,
            100.0,
            QuotaType::Credit,
            None,
        )];
        assert!(tracker
            .update(&provider, "Claude", &credits, &rules(), 0)
            .is_none());
        assert!(tracker
            .update(&provider, "Claude", &credits, &rules(), 0)
            .is_none());
    }

    #[test]
    fn points_quota_uses_amount_unit() {
        let mut tracker = AlertEngine::new();
        let provider = pid(ProviderKind::Claude);

        let points_normal = vec![QuotaInfo::with_details(
            "points",
            50.0,
            100.0,
            QuotaType::Points,
            None,
        )];
        tracker.update(&provider, "Claude", &points_normal, &rules(), 0);

        let points_low = vec![QuotaInfo::with_details(
            "points",
            90.0,
            100.0,
            QuotaType::Points,
            None,
        )];
        assert!(matches!(
            tracker.update(&provider, "Claude", &points_low, &rules(), 0),
            Some(QuotaNotificationEvent::LowQuota { .. })
        ));
    }

    #[test]
    fn invalid_measurement_does_not_advance_baseline() {
        let mut tracker = AlertEngine::new();
        let provider = pid(ProviderKind::Claude);

        tracker.update(&provider, "Claude", &remaining(50.0), &rules(), 0);

        let invalid = vec![QuotaInfo::with_details(
            "g",
            10.0,
            0.0,
            QuotaType::General,
            None,
        )];
        assert!(tracker
            .update(&provider, "Claude", &invalid, &rules(), 0)
            .is_none());

        assert!(matches!(
            tracker.update(&provider, "Claude", &remaining(8.0), &rules(), 0),
            Some(QuotaNotificationEvent::LowQuota { .. })
        ));
    }

    #[test]
    fn rebaseline_suppresses_duplicate_but_keeps_transitions() {
        let mut tracker = AlertEngine::new();
        let claude = pid(ProviderKind::Claude);

        tracker.update(&claude, "Claude", &remaining(30.0), &rules(), 0);

        tracker.rebaseline_alerts(&claude, &remaining(8.0), &rules());
        assert!(tracker
            .update(&claude, "Claude", &remaining(7.0), &rules(), 0)
            .is_none());

        assert!(matches!(
            tracker.update(&claude, "Claude", &remaining(0.0), &rules(), 0),
            Some(QuotaNotificationEvent::Exhausted { .. })
        ));
    }

    #[test]
    fn rebaseline_alerts_preserves_usage_baseline() {
        let mut tracker = AlertEngine::new();
        let claude = pid(ProviderKind::Claude);
        let mut new_rules = QuotaRules::default();
        new_rules.set(
            QuotaThresholdUnit::Percentage,
            QuotaThresholds {
                warning: 95.0,
                critical: 90.0,
                notify: 85.0,
            },
        );

        tracker.update(&claude, "Claude", &remaining(90.0), &rules(), 10);
        tracker.update(&claude, "Claude", &remaining(85.0), &rules(), 10);

        tracker.rebaseline_alerts(&claude, &remaining(85.0), &new_rules);

        let alert = tracker.update(&claude, "Claude", &remaining(80.0), &new_rules, 10);
        assert!(
            !matches!(alert, Some(QuotaNotificationEvent::LowQuota { .. })),
            "rebaseline 到 Low 不应在下一次更新补发 LowQuota: {alert:?}"
        );
        assert_usage_progress(alert, 80.0);

        assert!(matches!(
            tracker.update(&claude, "Claude", &remaining(0.0), &new_rules, 10),
            Some(QuotaNotificationEvent::Exhausted { .. })
        ));
        assert!(matches!(
            tracker.update(&claude, "Claude", &remaining(50.0), &new_rules, 10),
            Some(QuotaNotificationEvent::Recovered { .. })
        ));
    }

    #[test]
    fn rebaseline_alerts_without_entry_is_noop() {
        let mut tracker = AlertEngine::new();
        let claude = pid(ProviderKind::Claude);

        tracker.rebaseline_alerts(&claude, &remaining(5.0), &rules());

        assert!(tracker
            .update(&claude, "Claude", &remaining(5.0), &rules(), 0)
            .is_none());
    }

    #[test]
    fn rebaseline_alerts_without_valid_quotas_keeps_alert_state() {
        let mut tracker = AlertEngine::new();
        let claude = pid(ProviderKind::Claude);

        tracker.update(&claude, "Claude", &remaining(50.0), &rules(), 0);
        tracker.rebaseline_alerts(&claude, &[], &rules());

        assert!(matches!(
            tracker.update(&claude, "Claude", &remaining(5.0), &rules(), 0),
            Some(QuotaNotificationEvent::LowQuota { .. })
        ));
        assert!(matches!(
            tracker.update(&claude, "Claude", &remaining(0.0), &rules(), 0),
            Some(QuotaNotificationEvent::Exhausted { .. })
        ));
    }

    #[test]
    fn rebaseline_alerts_without_valid_quotas_keeps_usage_baseline() {
        let mut tracker = AlertEngine::new();
        let claude = pid(ProviderKind::Claude);

        tracker.update(&claude, "Claude", &remaining(90.0), &rules(), 10);
        tracker.update(&claude, "Claude", &remaining(85.0), &rules(), 10);
        tracker.rebaseline_alerts(&claude, &[], &rules());

        assert_usage_progress(
            tracker.update(&claude, "Claude", &remaining(80.0), &rules(), 10),
            80.0,
        );
    }

    #[test]
    fn test_usage_step_fires_at_step_boundaries() {
        let mut tracker = AlertEngine::new();
        let claude = pid(ProviderKind::Claude);

        assert!(tracker
            .update(&claude, "Claude", &remaining(83.0), &rules(), 5)
            .is_none());
        assert!(tracker
            .update(&claude, "Claude", &remaining(81.0), &rules(), 5)
            .is_none());
        assert!(tracker
            .update(&claude, "Claude", &remaining(79.0), &rules(), 5)
            .is_none());
        assert_usage_progress(
            tracker.update(&claude, "Claude", &remaining(78.0), &rules(), 5),
            78.0,
        );
        assert_usage_progress(
            tracker.update(&claude, "Claude", &remaining(73.0), &rules(), 5),
            73.0,
        );
    }

    #[test]
    fn test_usage_step_large_drop_fires_once() {
        let mut tracker = AlertEngine::new();
        let claude = pid(ProviderKind::Claude);

        tracker.update(&claude, "Claude", &remaining(73.0), &rules(), 5);

        assert_usage_progress(
            tracker.update(&claude, "Claude", &remaining(60.0), &rules(), 5),
            60.0,
        );
        assert!(tracker
            .update(&claude, "Claude", &remaining(59.0), &rules(), 5)
            .is_none());
        assert_usage_progress(
            tracker.update(&claude, "Claude", &remaining(55.0), &rules(), 5),
            55.0,
        );
    }

    #[test]
    fn test_usage_step_rebuilds_baseline_on_rebound() {
        let mut tracker = AlertEngine::new();
        let claude = pid(ProviderKind::Claude);

        tracker.update(&claude, "Claude", &remaining(83.0), &rules(), 5);
        tracker.update(&claude, "Claude", &remaining(81.0), &rules(), 5);
        assert!(tracker
            .update(&claude, "Claude", &remaining(82.0), &rules(), 5)
            .is_none());
        assert_usage_progress(
            tracker.update(&claude, "Claude", &remaining(77.0), &rules(), 5),
            77.0,
        );
    }

    #[test]
    fn test_usage_step_yields_to_threshold_alerts() {
        let mut tracker = AlertEngine::new();
        let claude = pid(ProviderKind::Claude);

        tracker.update(&claude, "Claude", &remaining(13.0), &rules(), 5);
        assert!(matches!(
            tracker.update(&claude, "Claude", &remaining(8.0), &rules(), 5),
            Some(QuotaNotificationEvent::LowQuota { quota, .. })
                if (quota.percent_remaining() - 8.0).abs() < 1e-9
        ));
        assert!(tracker
            .update(&claude, "Claude", &remaining(8.0), &rules(), 5)
            .is_none());
        assert_usage_progress(
            tracker.update(&claude, "Claude", &remaining(3.0), &rules(), 5),
            3.0,
        );
        assert!(matches!(
            tracker.update(&claude, "Claude", &remaining(0.0), &rules(), 5),
            Some(QuotaNotificationEvent::Exhausted { .. })
        ));
        assert!(matches!(
            tracker.update(&claude, "Claude", &remaining(50.0), &rules(), 5),
            Some(QuotaNotificationEvent::Recovered { .. })
        ));
        assert_usage_progress(
            tracker.update(&claude, "Claude", &remaining(45.0), &rules(), 5),
            45.0,
        );
    }

    #[test]
    fn test_usage_step_change_rebuilds_baseline() {
        let mut tracker = AlertEngine::new();
        let claude = pid(ProviderKind::Claude);

        tracker.update(&claude, "Claude", &remaining(100.0), &rules(), 5);
        tracker.update(&claude, "Claude", &remaining(97.0), &rules(), 5);

        assert!(tracker
            .update(&claude, "Claude", &remaining(95.0), &rules(), 10)
            .is_none());
        assert!(tracker
            .update(&claude, "Claude", &remaining(90.0), &rules(), 10)
            .is_none());
        assert_usage_progress(
            tracker.update(&claude, "Claude", &remaining(85.0), &rules(), 10),
            85.0,
        );
    }

    #[test]
    fn test_usage_step_zero_disables_progress() {
        let mut tracker = AlertEngine::new();
        let claude = pid(ProviderKind::Claude);

        tracker.update(&claude, "Claude", &remaining(83.0), &rules(), 0);
        assert!(tracker
            .update(&claude, "Claude", &remaining(78.0), &rules(), 0)
            .is_none());
        assert!(tracker
            .update(&claude, "Claude", &remaining(50.0), &rules(), 0)
            .is_none());
    }

    #[test]
    fn test_usage_step_empty_quotas_not_a_sample() {
        let mut tracker = AlertEngine::new();
        let claude = pid(ProviderKind::Claude);

        tracker.update(&claude, "Claude", &remaining(83.0), &rules(), 5);
        assert!(tracker
            .update(&claude, "Claude", &[], &rules(), 5)
            .is_none());
        assert_usage_progress(
            tracker.update(&claude, "Claude", &remaining(78.0), &rules(), 5),
            78.0,
        );
    }

    #[test]
    fn test_usage_step_balance_only_never_alerts() {
        let mut tracker = AlertEngine::new();
        let provider = pid(ProviderKind::Claude);

        assert!(tracker
            .update(&provider, "Claude", &balance_quota(5.0), &rules(), 5)
            .is_none());
        assert!(matches!(
            tracker.update(&provider, "Claude", &balance_quota(0.5), &rules(), 5),
            Some(QuotaNotificationEvent::LowQuota { .. })
        ));
        assert!(tracker
            .update(&provider, "Claude", &balance_quota(0.2), &rules(), 5)
            .is_none());
    }

    #[test]
    fn test_usage_step_skips_invalid_measurement_quotas() {
        let mut tracker = AlertEngine::new();
        let claude = pid(ProviderKind::Claude);
        let quotas = |used_ok: f64| {
            vec![
                QuotaInfo::with_details("general", used_ok, 100.0, QuotaType::General, None),
                QuotaInfo::with_details("broken", f64::NAN, 100.0, QuotaType::General, None),
                QuotaInfo::balance_only("credits", 5.0, None, QuotaType::Credit, None),
            ]
        };

        assert!(tracker
            .update(&claude, "Claude", &quotas(40.0), &rules(), 5)
            .is_none());
        assert_usage_progress(
            tracker.update(&claude, "Claude", &quotas(45.0), &rules(), 5),
            55.0,
        );
    }

    #[test]
    fn test_usage_step_uses_worst_quota() {
        let mut tracker = AlertEngine::new();
        let claude = pid(ProviderKind::Claude);

        let first = vec![
            QuotaInfo::new("a", 20.0, 100.0),
            QuotaInfo::new("b", 70.0, 100.0),
        ];
        let second = vec![
            QuotaInfo::new("a", 25.0, 100.0),
            QuotaInfo::new("b", 76.0, 100.0),
        ];

        assert!(tracker
            .update(&claude, "Claude", &first, &rules(), 5)
            .is_none());
        assert_usage_progress(
            tracker.update(&claude, "Claude", &second, &rules(), 5),
            24.0,
        );
    }

    #[test]
    fn test_usage_step_independent_providers() {
        let mut tracker = AlertEngine::new();
        let claude = pid(ProviderKind::Claude);
        let gemini = pid(ProviderKind::Gemini);

        tracker.update(&claude, "Claude", &remaining(83.0), &rules(), 5);
        tracker.update(&gemini, "Gemini", &remaining(90.0), &rules(), 5);

        assert_usage_progress(
            tracker.update(&claude, "Claude", &remaining(78.0), &rules(), 5),
            78.0,
        );
        assert!(tracker
            .update(&gemini, "Gemini", &remaining(86.0), &rules(), 5)
            .is_none());
        assert_usage_progress_for(
            tracker.update(&gemini, "Gemini", &remaining(85.0), &rules(), 5),
            "Gemini",
            85.0,
        );
    }

    #[test]
    fn test_reset_usage_clears_baseline_but_keeps_alert_state() {
        let mut tracker = AlertEngine::new();
        let claude = pid(ProviderKind::Claude);

        tracker.update(&claude, "Claude", &remaining(83.0), &rules(), 5);
        tracker.reset_usage(&claude);
        assert!(tracker
            .update(&claude, "Claude", &remaining(78.0), &rules(), 5)
            .is_none());
        assert_usage_progress(
            tracker.update(&claude, "Claude", &remaining(73.0), &rules(), 5),
            73.0,
        );

        assert!(matches!(
            tracker.update(&claude, "Claude", &remaining(8.0), &rules(), 5),
            Some(QuotaNotificationEvent::LowQuota { .. })
        ));
        tracker.reset_usage(&claude);
        assert!(matches!(
            tracker.update(&claude, "Claude", &remaining(0.0), &rules(), 5),
            Some(QuotaNotificationEvent::Exhausted { .. })
        ));
        assert!(matches!(
            tracker.update(&claude, "Claude", &remaining(50.0), &rules(), 5),
            Some(QuotaNotificationEvent::Recovered { .. })
        ));
    }

    #[test]
    fn test_reset_all_usage_clears_all_baselines() {
        let mut tracker = AlertEngine::new();
        let claude = pid(ProviderKind::Claude);
        let gemini = pid(ProviderKind::Gemini);

        tracker.update(&claude, "Claude", &remaining(83.0), &rules(), 5);
        tracker.update(&gemini, "Gemini", &remaining(90.0), &rules(), 5);
        tracker.reset_all_usage();

        assert!(tracker
            .update(&claude, "Claude", &remaining(78.0), &rules(), 5)
            .is_none());
        assert!(tracker
            .update(&gemini, "Gemini", &remaining(85.0), &rules(), 5)
            .is_none());

        assert!(matches!(
            tracker.update(&claude, "Claude", &remaining(8.0), &rules(), 5),
            Some(QuotaNotificationEvent::LowQuota { .. })
        ));
        assert!(matches!(
            tracker.update(&gemini, "Gemini", &remaining(7.0), &rules(), 5),
            Some(QuotaNotificationEvent::LowQuota { .. })
        ));
        tracker.reset_all_usage();
        assert!(matches!(
            tracker.update(&claude, "Claude", &remaining(0.0), &rules(), 5),
            Some(QuotaNotificationEvent::Exhausted { .. })
        ));
        assert!(matches!(
            tracker.update(&gemini, "Gemini", &remaining(0.0), &rules(), 5),
            Some(QuotaNotificationEvent::Exhausted { .. })
        ));
    }

    #[test]
    fn test_remove_clears_provider_state() {
        let mut tracker = AlertEngine::new();
        let claude = pid(ProviderKind::Claude);

        tracker.update(&claude, "Claude", &remaining(13.0), &rules(), 5);
        assert!(matches!(
            tracker.update(&claude, "Claude", &remaining(8.0), &rules(), 5),
            Some(QuotaNotificationEvent::LowQuota { .. })
        ));
        tracker.remove(&claude);

        assert!(tracker
            .update(&claude, "Claude", &remaining(8.0), &rules(), 5)
            .is_none());
    }

    #[test]
    fn balance_low_exhausted_recovered_sequence_stays_strict() {
        let mut tracker = AlertEngine::new();
        let provider = pid(ProviderKind::Claude);

        assert!(tracker
            .update(&provider, "Claude", &balance_quota(2.0), &rules(), 0)
            .is_none());
        assert!(matches!(
            tracker.update(&provider, "Claude", &balance_quota(1e-15), &rules(), 0),
            Some(QuotaNotificationEvent::LowQuota { .. })
        ));
        assert!(matches!(
            tracker.update(&provider, "Claude", &balance_quota(0.0), &rules(), 0),
            Some(QuotaNotificationEvent::Exhausted { .. })
        ));
        assert!(matches!(
            tracker.update(&provider, "Claude", &balance_quota(1e-15), &rules(), 0),
            Some(QuotaNotificationEvent::Recovered { .. })
        ));
    }
}
