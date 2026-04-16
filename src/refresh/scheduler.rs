//! 纯调度决策引擎 — 无 async、无 IO，完全可同步测试。
//!
//! `RefreshScheduler` 封装了周期性刷新的定时策略和 per-provider 的
//! cooldown / in-flight 状态管理，供 `RefreshCoordinator` 使用。

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::models::{ProviderId, SystemSettings};

use super::types::{RefreshReason, RefreshResult};

/// 最小 cooldown 时间（秒），防止过于频繁的刷新
const MIN_COOLDOWN_SECS: u64 = 30;
/// 自动刷新禁用时的检查间隔（秒）
const DISABLED_CHECK_INTERVAL_SECS: u64 = 3600;
/// 默认刷新间隔（分钟）— 与 SystemSettings::DEFAULT_REFRESH_INTERVAL_MINS 保持一致
const DEFAULT_INTERVAL_MINS: u64 = SystemSettings::DEFAULT_REFRESH_INTERVAL_MINS;

/// 纯调度决策引擎
pub(super) struct RefreshScheduler {
    /// Per-provider 上次成功刷新时间
    last_refreshed: HashMap<ProviderId, Instant>,
    /// Per-provider 是否正在刷新
    in_flight: HashMap<ProviderId, bool>,
    /// 刷新间隔（分钟），0 = 禁用
    interval_mins: u64,
    /// 已启用的 Provider 列表
    enabled_providers: Vec<ProviderId>,
    /// 下一次周期性刷新的绝对时间点
    next_periodic: Instant,
}

impl RefreshScheduler {
    pub fn new() -> Self {
        Self {
            last_refreshed: HashMap::new(),
            in_flight: HashMap::new(),
            interval_mins: DEFAULT_INTERVAL_MINS,
            enabled_providers: Vec::new(),
            next_periodic: Instant::now() + Duration::from_secs(DEFAULT_INTERVAL_MINS * 60),
        }
    }

    // ========================================================================
    // 查询方法
    // ========================================================================

    pub fn interval_mins(&self) -> u64 {
        self.interval_mins
    }

    pub fn enabled_providers(&self) -> &[ProviderId] {
        &self.enabled_providers
    }

    /// 周期性刷新间隔（考虑禁用状态）
    pub fn periodic_duration(&self) -> Duration {
        if self.interval_mins > 0 {
            Duration::from_secs(self.interval_mins * 60)
        } else {
            Duration::from_secs(DISABLED_CHECK_INTERVAL_SECS)
        }
    }

    /// Cooldown 时长：刷新间隔的一半，最小 MIN_COOLDOWN_SECS
    pub fn cooldown(&self) -> Duration {
        let interval_secs = self.interval_mins * 60;
        let half = interval_secs / 2;
        Duration::from_secs(half.max(MIN_COOLDOWN_SECS))
    }

    /// 距下一次周期刷新的等待时间
    pub fn time_until_next_periodic(&self) -> Duration {
        self.next_periodic.saturating_duration_since(Instant::now())
    }

    /// 自动刷新是否已禁用
    pub fn is_auto_refresh_disabled(&self) -> bool {
        self.interval_mins == 0
    }

    // ========================================================================
    // 调度判定
    // ========================================================================

    /// 检查 Provider 是否在 cooldown 期内（距上次成功刷新未超过 cooldown 时长）
    pub fn is_on_cooldown(&self, id: &ProviderId) -> bool {
        if let Some(instant) = self.last_refreshed.get(id) {
            instant.elapsed() < self.cooldown()
        } else {
            false
        }
    }

    /// 检查 Provider 是否正在刷新
    fn is_in_flight(&self, id: &ProviderId) -> bool {
        self.in_flight.get(id).copied().unwrap_or(false)
    }

    /// 检查 Provider 是否有资格被刷新，返回跳过原因（None = 可以刷新）
    pub fn check_eligibility(
        &self,
        id: &ProviderId,
        reason: RefreshReason,
    ) -> Option<RefreshResult> {
        if !self.enabled_providers.contains(id) {
            return Some(RefreshResult::SkippedDisabled);
        }
        if self.is_in_flight(id) {
            return Some(RefreshResult::SkippedInFlight);
        }
        if matches!(reason, RefreshReason::Periodic | RefreshReason::Startup)
            && self.is_on_cooldown(id)
        {
            log::info!(target: "refresh", "skipping {} (cooldown)", id);
            return Some(RefreshResult::SkippedCooldown);
        }
        None
    }

    // ========================================================================
    // 状态变更
    // ========================================================================

    /// 标记 Provider 开始刷新
    pub fn mark_in_flight(&mut self, id: &ProviderId) {
        self.in_flight.insert(id.clone(), true);
    }

    /// 标记 Provider 刷新完成（清除 in-flight 标志）
    pub fn clear_in_flight(&mut self, id: &ProviderId) {
        self.in_flight.insert(id.clone(), false);
    }

    /// 记录一次成功刷新（更新 last_refreshed）
    pub fn record_success(&mut self, id: &ProviderId) {
        self.last_refreshed.insert(id.clone(), Instant::now());
    }

    /// 更新配置（刷新间隔 + 启用列表）。仅在间隔实际变化时重置 deadline。
    pub fn update_config(&mut self, interval_mins: u64, enabled: Vec<ProviderId>) {
        let interval_changed = self.interval_mins != interval_mins;
        log::info!(
            target: "refresh",
            "config updated: interval={}min, {} providers enabled",
            interval_mins,
            enabled.len()
        );
        self.interval_mins = interval_mins;
        self.enabled_providers = enabled;
        if interval_changed {
            self.next_periodic = Instant::now() + self.periodic_duration();
        }
    }

    /// 推进周期 deadline 到下一个周期
    pub fn advance_periodic_deadline(&mut self) {
        self.next_periodic = Instant::now() + self.periodic_duration();
    }

    /// 自动刷新禁用时推进 deadline
    pub fn advance_disabled_deadline(&mut self) {
        self.next_periodic = Instant::now() + Duration::from_secs(DISABLED_CHECK_INTERVAL_SECS);
    }

    /// 清理已不存在的 Provider 的残留状态
    pub fn cleanup_stale(&mut self, valid_ids: &std::collections::HashSet<&ProviderId>) {
        self.last_refreshed.retain(|id, _| valid_ids.contains(id));
        self.in_flight.retain(|id, _| valid_ids.contains(id));
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ProviderKind;

    fn make_scheduler(interval_mins: u64) -> RefreshScheduler {
        let mut s = RefreshScheduler::new();
        s.interval_mins = interval_mins;
        s
    }

    fn claude_id() -> ProviderId {
        ProviderId::BuiltIn(ProviderKind::Claude)
    }

    // -- periodic_duration --

    #[test]
    fn test_periodic_duration_enabled() {
        let s = make_scheduler(5);
        assert_eq!(s.periodic_duration(), Duration::from_secs(300));
    }

    #[test]
    fn test_periodic_duration_disabled() {
        let s = make_scheduler(0);
        assert_eq!(
            s.periodic_duration(),
            Duration::from_secs(DISABLED_CHECK_INTERVAL_SECS)
        );
    }

    // -- cooldown --

    #[test]
    fn test_cooldown_is_half_interval() {
        let s = make_scheduler(10);
        // 10 min = 600s, half = 300s
        assert_eq!(s.cooldown(), Duration::from_secs(300));
    }

    #[test]
    fn test_cooldown_minimum_30s() {
        let s = make_scheduler(0);
        assert_eq!(s.cooldown(), Duration::from_secs(MIN_COOLDOWN_SECS));

        let s = make_scheduler(1);
        // 1 min = 60s, half = 30s
        assert_eq!(s.cooldown(), Duration::from_secs(30));
    }

    // -- is_on_cooldown --

    #[test]
    fn test_is_on_cooldown_no_history() {
        let s = make_scheduler(5);
        assert!(!s.is_on_cooldown(&claude_id()));
    }

    #[test]
    fn test_is_on_cooldown_just_refreshed() {
        let mut s = make_scheduler(5);
        s.last_refreshed.insert(claude_id(), Instant::now());
        assert!(s.is_on_cooldown(&claude_id()));
    }

    #[test]
    fn test_is_on_cooldown_expired() {
        let mut s = make_scheduler(5);
        // 3 分钟前刷新，cooldown = 2.5 min = 150s，已过期
        s.last_refreshed
            .insert(claude_id(), Instant::now() - Duration::from_secs(180));
        assert!(!s.is_on_cooldown(&claude_id()));
    }

    // -- check_eligibility --

    #[test]
    fn test_eligibility_disabled_provider() {
        let s = make_scheduler(5);
        assert!(matches!(
            s.check_eligibility(&claude_id(), RefreshReason::Periodic),
            Some(RefreshResult::SkippedDisabled)
        ));
    }

    #[test]
    fn test_eligibility_in_flight() {
        let mut s = make_scheduler(5);
        s.enabled_providers.push(claude_id());
        s.in_flight.insert(claude_id(), true);
        assert!(matches!(
            s.check_eligibility(&claude_id(), RefreshReason::Periodic),
            Some(RefreshResult::SkippedInFlight)
        ));
    }

    #[test]
    fn test_eligibility_periodic_on_cooldown() {
        let mut s = make_scheduler(5);
        s.enabled_providers.push(claude_id());
        s.last_refreshed.insert(claude_id(), Instant::now());
        assert!(matches!(
            s.check_eligibility(&claude_id(), RefreshReason::Periodic),
            Some(RefreshResult::SkippedCooldown)
        ));
    }

    #[test]
    fn test_eligibility_manual_ignores_cooldown() {
        let mut s = make_scheduler(5);
        s.enabled_providers.push(claude_id());
        s.last_refreshed.insert(claude_id(), Instant::now());
        assert!(s
            .check_eligibility(&claude_id(), RefreshReason::Manual)
            .is_none());
    }

    #[test]
    fn test_eligibility_provider_toggled_ignores_cooldown() {
        let mut s = make_scheduler(5);
        s.enabled_providers.push(claude_id());
        s.last_refreshed.insert(claude_id(), Instant::now());
        assert!(s
            .check_eligibility(&claude_id(), RefreshReason::ProviderToggled)
            .is_none());
    }

    #[test]
    fn test_eligibility_ok_when_eligible() {
        let mut s = make_scheduler(5);
        s.enabled_providers.push(claude_id());
        assert!(s
            .check_eligibility(&claude_id(), RefreshReason::Periodic)
            .is_none());
    }

    // -- update_config --

    #[test]
    fn test_update_config_resets_deadline_on_interval_change() {
        let mut s = make_scheduler(5);
        let before = Instant::now();
        s.update_config(10, vec![]);
        let after = Instant::now();
        // deadline 应在 now + 10min 附近
        assert!(s.next_periodic >= before + Duration::from_secs(600));
        assert!(s.next_periodic <= after + Duration::from_secs(600));
    }

    #[test]
    fn test_update_config_preserves_deadline_on_same_interval() {
        let mut s = make_scheduler(5);
        let original_deadline = s.next_periodic;
        s.update_config(5, vec![claude_id()]);
        // interval 未变 → deadline 不变
        assert_eq!(s.next_periodic, original_deadline);
    }

    // -- cleanup_stale --

    #[test]
    fn test_cleanup_stale_removes_unknown_ids() {
        let mut s = make_scheduler(5);
        let keep = claude_id();
        let remove = ProviderId::BuiltIn(ProviderKind::Copilot);
        s.last_refreshed.insert(keep.clone(), Instant::now());
        s.last_refreshed.insert(remove.clone(), Instant::now());
        s.in_flight.insert(keep.clone(), false);
        s.in_flight.insert(remove.clone(), true);

        let valid: std::collections::HashSet<_> = [&keep].into_iter().collect();
        s.cleanup_stale(&valid);

        assert!(s.last_refreshed.contains_key(&keep));
        assert!(!s.last_refreshed.contains_key(&remove));
        assert!(s.in_flight.contains_key(&keep));
        assert!(!s.in_flight.contains_key(&remove));
    }

    // -- advance_periodic_deadline --

    #[test]
    fn test_advance_periodic_deadline() {
        let mut s = make_scheduler(5);
        let before = Instant::now();
        s.advance_periodic_deadline();
        let after = Instant::now();
        assert!(s.next_periodic >= before + Duration::from_secs(300));
        assert!(s.next_periodic <= after + Duration::from_secs(300));
    }

    // -- initial state --

    #[test]
    fn test_new_scheduler_sets_initial_deadline() {
        let before = Instant::now();
        let s = RefreshScheduler::new();
        let after = Instant::now();
        assert!(s.next_periodic >= before + Duration::from_secs(300));
        assert!(s.next_periodic <= after + Duration::from_secs(300));
        assert_eq!(s.interval_mins(), 5);
    }
}
