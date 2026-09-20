//! App-only cross-layer lifecycle timing policy.
//!
//! These values are intentionally named by the protocol they govern. Keeping
//! them together makes changes to shutdown, persistence, and window-opening
//! behavior reviewable without changing the behavior of unrelated timers.

use std::time::Duration;

/// Maximum time allowed for foreground workers to drain during app shutdown.
pub(crate) const APP_SHUTDOWN_DEADLINE: Duration = Duration::from_millis(60);

/// Grace period for the Linux D-Bus service worker during handle drop.
#[cfg(target_os = "linux")]
pub(crate) const DBUS_SHUTDOWN_GRACE_PERIOD: Duration = Duration::from_millis(20);

/// Debounce window used to coalesce settings writes.
pub(crate) const SETTINGS_WRITE_DEBOUNCE: Duration = Duration::from_millis(500);

/// Fallback grace period when a settings writer is dropped outside normal app shutdown.
pub(crate) const SETTINGS_WRITER_SHUTDOWN_GRACE_PERIOD: Duration = Duration::from_millis(80);

/// Delay between closing the tray popup and opening the settings window.
pub(crate) const SETTINGS_WINDOW_OPEN_DELAY: Duration = Duration::from_millis(10);
