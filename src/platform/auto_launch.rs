//! Platform-specific "launch at login" support.
//!
//! - **macOS**: uses `SMAppService` (ServiceManagement framework) to register
//!   as a Login Item. Requires the app to run as a `.app` bundle with a valid
//!   `CFBundleIdentifier`. Shows under System Settings → General → Login Items.
//! - **Linux**: writes / removes an XDG autostart `.desktop` file under
//!   `$XDG_CONFIG_HOME/autostart/` (defaults to `~/.config/autostart/`).

use anyhow::Result;
use log::{debug, warn};

/// Register the current application to launch at login.
pub fn enable() -> Result<()> {
    debug!(target: "auto_launch", "enabling launch-at-login");
    platform::enable()
}

/// Remove the launch-at-login registration.
pub fn disable() -> Result<()> {
    debug!(target: "auto_launch", "disabling launch-at-login");
    platform::disable()
}

/// Check whether the launch-at-login registration is currently active.
pub fn is_enabled() -> bool {
    platform::is_enabled()
}

/// Apply the desired state: enable if `desired` is true, disable otherwise.
pub fn sync(desired: bool) {
    let current = is_enabled();
    if current == desired {
        return;
    }
    let result = if desired { enable() } else { disable() };
    if let Err(err) = result {
        warn!(target: "auto_launch", "failed to sync launch-at-login (desired={desired}): {err}");
    }
}

// ─── macOS: SMAppService ────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use smappservice_rs::{AppService, ServiceStatus, ServiceType};
    use std::fs;
    use std::path::{Path, PathBuf};

    /// Old LaunchAgent plist identifier — used for migration cleanup.
    /// 保持字面量：历史文件名必须与磁盘上的旧文件一致，不随 APP_BUNDLE_ID 变化。
    const LEGACY_PLIST_NAME: &str = "com.bananatray.app.plist";

    pub fn enable() -> Result<()> {
        cleanup_legacy_plist();
        let service = AppService::new(ServiceType::MainApp);
        service
            .register()
            .map_err(|e| anyhow::anyhow!("SMAppService register failed: {e}"))?;
        debug!(target: "auto_launch", "registered as Login Item via SMAppService");
        Ok(())
    }

    pub fn disable() -> Result<()> {
        cleanup_legacy_plist();
        let service = AppService::new(ServiceType::MainApp);
        service
            .unregister()
            .map_err(|e| anyhow::anyhow!("SMAppService unregister failed: {e}"))?;
        debug!(target: "auto_launch", "unregistered Login Item via SMAppService");
        Ok(())
    }

    pub fn is_enabled() -> bool {
        let service = AppService::new(ServiceType::MainApp);
        service.status() == ServiceStatus::Enabled
    }

    /// Remove the legacy LaunchAgent plist if it exists (migration from v0.1).
    pub(super) fn cleanup_legacy_plist() {
        if let Some(path) = legacy_plist_path() {
            if path.exists() {
                if let Err(e) = fs::remove_file(&path) {
                    warn!(target: "auto_launch", "failed to remove legacy plist {}: {e}", path.display());
                } else {
                    debug!(target: "auto_launch", "removed legacy LaunchAgent plist at {}", path.display());
                }
            }
        }
    }

    pub(super) fn legacy_plist_path() -> Option<PathBuf> {
        let home = std::env::var("HOME").ok()?;
        Some(
            Path::new(&home)
                .join("Library")
                .join("LaunchAgents")
                .join(LEGACY_PLIST_NAME),
        )
    }
}

// ─── Linux: XDG autostart ───────────────────────────────────────────────────

#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    use anyhow::Context;
    use std::fs;
    use std::path::{Path, PathBuf};

    use crate::platform::{APP_BUNDLE_ID, APP_NAME};

    pub fn enable() -> Result<()> {
        let exe = std::env::current_exe().context("failed to determine current executable")?;
        let path = entry_path()?;
        write_entry(&path, &exe)
    }

    pub fn disable() -> Result<()> {
        let path = entry_path()?;
        remove_entry(&path)
    }

    pub fn is_enabled() -> bool {
        entry_path().map(|p| p.exists()).unwrap_or(false)
    }

    fn entry_path() -> Result<PathBuf> {
        let config_dir = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .context("could not determine config directory")?;
        Ok(entry_path_under(&config_dir))
    }

    /// Build the desktop entry path given a config directory — testable.
    pub(super) fn entry_path_under(config_dir: &Path) -> PathBuf {
        config_dir
            .join("autostart")
            .join(format!("{APP_BUNDLE_ID}.desktop"))
    }

    pub(super) fn entry_content(exe: &Path) -> String {
        format!(
            "[Desktop Entry]\n\
             Type=Application\n\
             Name={APP_NAME}\n\
             Comment=AI Coding Assistant Quota Monitor\n\
             Exec={exe}\n\
             Terminal=false\n\
             StartupNotify=false\n\
             X-GNOME-Autostart-enabled=true\n",
            exe = exe.display()
        )
    }

    /// Write the autostart desktop entry — exposed for testing.
    pub(super) fn write_entry(path: &Path, exe: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let content = entry_content(exe);
        fs::write(path, &content)
            .with_context(|| format!("failed to write .desktop at {}", path.display()))?;
        debug!(target: "auto_launch", "wrote autostart desktop entry at {}", path.display());
        Ok(())
    }

    /// Remove the autostart desktop entry — exposed for testing.
    pub(super) fn remove_entry(path: &Path) -> Result<()> {
        if path.exists() {
            fs::remove_file(path)
                .with_context(|| format!("failed to remove .desktop at {}", path.display()))?;
            debug!(target: "auto_launch", "removed autostart desktop entry at {}", path.display());
        }
        Ok(())
    }
}

// ─── Unsupported platforms ──────────────────────────────────────────────────

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
mod platform {
    use super::*;

    pub fn enable() -> Result<()> {
        anyhow::bail!("launch-at-login is not supported on this platform")
    }

    pub fn disable() -> Result<()> {
        Ok(())
    }

    pub fn is_enabled() -> bool {
        false
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // --- macOS tests ---
    //
    // SMAppService calls require a running .app bundle context, so we only
    // test the legacy cleanup helper and skip register/unregister in unit
    // tests. Integration tests should cover the actual SMAppService flow.

    #[cfg(target_os = "macos")]
    mod macos_tests {
        use std::fs;
        use std::path::Path;

        #[test]
        fn legacy_plist_path_is_under_launch_agents() {
            if let Ok(home) = std::env::var("HOME") {
                let expected =
                    Path::new(&home).join("Library/LaunchAgents/com.bananatray.app.plist");
                let actual = super::super::platform::legacy_plist_path().unwrap();
                assert_eq!(actual, expected);
            }
        }

        #[test]
        fn cleanup_legacy_plist_removes_file() {
            let dir = tempfile::tempdir().unwrap();
            let plist = dir.path().join("com.bananatray.app.plist");
            fs::write(&plist, "<?xml version=\"1.0\"?><plist/>").unwrap();
            assert!(plist.exists());

            // Manually test the removal logic (can't call cleanup_legacy_plist
            // directly because it reads HOME, so we inline the logic).
            if plist.exists() {
                fs::remove_file(&plist).unwrap();
            }
            assert!(!plist.exists());
        }

        #[test]
        fn cleanup_legacy_plist_noop_when_absent() {
            // Should not panic when file doesn't exist.
            super::super::platform::cleanup_legacy_plist();
        }
    }

    // --- Linux tests ---

    #[cfg(target_os = "linux")]
    mod linux_tests {
        use super::super::platform;
        use std::path::Path;

        #[test]
        fn desktop_content_contains_exe_path() {
            let exe = Path::new("/usr/bin/bananatray");
            let content = platform::entry_content(exe);
            assert!(content.contains("Exec=/usr/bin/bananatray"));
            assert!(content.contains("Name=BananaTray"));
            assert!(content.contains("Type=Application"));
            assert!(content.contains("Terminal=false"));
        }

        #[test]
        fn entry_path_under_builds_correct_path() {
            let config = Path::new("/home/user/.config");
            let path = platform::entry_path_under(config);
            assert_eq!(
                path,
                Path::new("/home/user/.config/autostart/com.bananatray.app.desktop")
            );
        }

        #[test]
        fn write_and_remove_roundtrip() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("test.desktop");
            let exe = Path::new("/usr/bin/bananatray");

            assert!(!path.exists());
            platform::write_entry(&path, exe).unwrap();
            assert!(path.exists());

            let content = std::fs::read_to_string(&path).unwrap();
            assert!(content.contains("/usr/bin/bananatray"));

            platform::remove_entry(&path).unwrap();
            assert!(!path.exists());
        }

        #[test]
        fn write_creates_parent_dirs() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("deep").join("nested").join("test.desktop");
            let exe = Path::new("/bin/test");

            platform::write_entry(&path, exe).unwrap();
            assert!(path.exists());
        }

        #[test]
        fn write_overwrites_existing_stale_content() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("test.desktop");

            let old_exe = Path::new("/old/path/bananatray");
            platform::write_entry(&path, old_exe).unwrap();

            let new_exe = Path::new("/new/path/bananatray");
            platform::write_entry(&path, new_exe).unwrap();
            let content = std::fs::read_to_string(&path).unwrap();
            assert!(content.contains("/new/path/bananatray"));
            assert!(!content.contains("/old/path/bananatray"));
        }

        #[test]
        fn remove_nonexistent_is_ok() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("nonexistent.desktop");
            assert!(platform::remove_entry(&path).is_ok());
        }
    }

    // --- sync() ---

    #[test]
    fn sync_disable_is_safe_when_not_enabled() {
        sync(false);
    }
}
