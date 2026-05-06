//! Popup activation state machine.
//!
//! The tracker keeps focus flicker from being interpreted as an intentional
//! close request while a tray popup is being mapped by the window manager.

use log::debug;
use std::time::Duration;

#[derive(Debug, Clone)]
pub(super) struct PopupActivationTracker {
    /// 窗口创建时间，用于计算 grace period
    created_at: std::time::Instant,
    /// 收到的 activation 事件计数（用于区分初始闪烁和真实交互）
    event_count: u32,
    /// 窗口是否曾经处于激活状态
    has_been_active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PopupActivationDecision {
    KeepOpen,
    Close,
    RecheckAfter(Duration),
}

/// 窗口创建后的保护期：在此期间内忽略 deactivation 事件。
/// Wayland 上焦点抖动通常在 200ms 内完成，600ms 留足余量。
pub(super) const GRACE_PERIOD: Duration = Duration::from_millis(600);

impl Default for PopupActivationTracker {
    fn default() -> Self {
        Self {
            created_at: std::time::Instant::now(),
            event_count: 0,
            has_been_active: false,
        }
    }
}

impl PopupActivationTracker {
    pub(super) fn on_activation_event(
        &mut self,
        is_active: bool,
        should_auto_hide: bool,
    ) -> PopupActivationDecision {
        self.event_count += 1;

        if is_active {
            self.has_been_active = true;
            return PopupActivationDecision::KeepOpen;
        }

        // 保护期内忽略 deactivation——Wayland compositor 在窗口创建阶段
        // 可能发出快速 focus→unfocus 抖动，不应解释为用户离开窗口。
        if let Some(remaining) = GRACE_PERIOD.checked_sub(self.created_at.elapsed()) {
            debug!(
                target: "tray",
                "ignoring deactivation during grace period (event #{}, elapsed={:?})",
                self.event_count,
                self.created_at.elapsed(),
            );
            return PopupActivationDecision::RecheckAfter(remaining);
        }

        if should_auto_hide && self.has_been_active {
            PopupActivationDecision::Close
        } else {
            PopupActivationDecision::KeepOpen
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{PopupActivationDecision, PopupActivationTracker, GRACE_PERIOD};
    use std::time::Instant;

    /// 创建一个已过保护期的 tracker，用于测试 auto-hide 逻辑
    fn tracker_past_grace() -> PopupActivationTracker {
        PopupActivationTracker {
            created_at: Instant::now() - GRACE_PERIOD - std::time::Duration::from_millis(100),
            event_count: 0,
            has_been_active: false,
        }
    }

    #[test]
    fn grace_period_blocks_immediate_deactivation() {
        // 模拟 Wayland 焦点抖动：窗口刚创建就收到 active→inactive
        let mut tracker = PopupActivationTracker::default();

        assert_eq!(
            tracker.on_activation_event(true, true),
            PopupActivationDecision::KeepOpen
        ); // 获得焦点
        assert!(matches!(
            tracker.on_activation_event(false, true),
            PopupActivationDecision::RecheckAfter(_)
        )); // 立即失焦——在保护期内，不关闭
    }

    #[test]
    fn auto_hide_requires_popup_to_have_been_active_first() {
        let mut tracker = tracker_past_grace();

        assert_eq!(
            tracker.on_activation_event(false, true),
            PopupActivationDecision::KeepOpen
        );
        assert_eq!(
            tracker.on_activation_event(false, true),
            PopupActivationDecision::KeepOpen
        );
    }

    #[test]
    fn auto_hide_closes_after_popup_loses_focus_post_activation() {
        let mut tracker = tracker_past_grace();

        assert_eq!(
            tracker.on_activation_event(true, true),
            PopupActivationDecision::KeepOpen
        );
        assert_eq!(
            tracker.on_activation_event(false, true),
            PopupActivationDecision::Close
        );
    }

    #[test]
    fn auto_hide_closes_after_late_activation_then_blur() {
        let mut tracker = tracker_past_grace();

        assert_eq!(
            tracker.on_activation_event(false, true),
            PopupActivationDecision::KeepOpen
        );
        assert_eq!(
            tracker.on_activation_event(true, true),
            PopupActivationDecision::KeepOpen
        );
        assert_eq!(
            tracker.on_activation_event(false, true),
            PopupActivationDecision::Close
        );
    }

    #[test]
    fn auto_hide_respects_setting_after_activation() {
        let mut tracker = tracker_past_grace();

        assert_eq!(
            tracker.on_activation_event(true, false),
            PopupActivationDecision::KeepOpen
        );
        assert_eq!(
            tracker.on_activation_event(false, false),
            PopupActivationDecision::KeepOpen
        );
    }
}
