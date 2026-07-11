//! Platform-specific "launch at login" support.
//!
//! - **macOS**: uses `SMAppService` (ServiceManagement framework) to register
//!   as a Login Item. Requires the app to run as a `.app` bundle with a valid
//!   `CFBundleIdentifier`. Shows under System Settings → General → Login Items.
//! - **Linux**: writes / removes an XDG autostart `.desktop` file under
//!   `$XDG_CONFIG_HOME/autostart/` (defaults to `~/.config/autostart/`).

use anyhow::Result;
use log::{debug, warn};
use std::sync::{mpsc, Mutex, OnceLock};

static SYNC_WORKER: OnceLock<Mutex<Option<mpsc::Sender<SyncRequest>>>> = OnceLock::new();

#[derive(Debug)]
struct SyncRequest {
    desired: bool,
    completion: Option<mpsc::Sender<()>>,
}

impl SyncRequest {
    fn background(desired: bool) -> Self {
        Self {
            desired,
            completion: None,
        }
    }

    fn with_completion(desired: bool, completion: mpsc::Sender<()>) -> Self {
        Self {
            desired,
            completion: Some(completion),
        }
    }
}

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
    sync_with(desired, is_enabled, |desired| {
        if desired {
            enable()
        } else {
            disable()
        }
    });
}

/// 在单一后台 worker 上应用 launch-at-login 状态。
///
/// worker 会在每次执行前合并排队请求，只应用当时最新的目标状态；如果执行期间又有
/// 新请求，则当前操作完成后继续应用最新状态，保证最终状态不会被较早的慢操作覆盖。
pub fn schedule_sync(desired: bool) {
    if let Err(err) = submit_sync_request(SyncRequest::background(desired)) {
        warn!(target: "auto_launch", "failed to schedule launch-at-login sync: {err}");
    }
}

/// 提交最终目标状态，并等待同一个后台 worker 完成应用。
///
/// 正常退出时用它建立屏障，避免进程在最后一次 launch-at-login 操作完成前结束。
pub fn sync_and_wait(desired: bool) {
    let (completion_tx, completion_rx) = mpsc::channel();
    if let Err(err) = submit_sync_request(SyncRequest::with_completion(desired, completion_tx)) {
        warn!(target: "auto_launch", "failed to schedule final launch-at-login sync: {err}");
        return;
    }

    if completion_rx.recv().is_err() {
        warn!(target: "auto_launch", "auto-launch worker stopped before completing final sync");
    }
}

fn submit_sync_request(mut request: SyncRequest) -> Result<()> {
    let worker = SYNC_WORKER.get_or_init(|| Mutex::new(None));
    let mut sender_slot = worker
        .lock()
        .map_err(|_| anyhow::anyhow!("auto-launch worker lock poisoned"))?;

    if let Some(sender) = sender_slot.as_ref() {
        match sender.send(request) {
            Ok(()) => return Ok(()),
            Err(err) => request = err.0,
        }
    }

    let sender = spawn_sync_worker(sync)?;
    sender
        .send(request)
        .map_err(|_| anyhow::anyhow!("auto-launch worker stopped before accepting a request"))?;
    *sender_slot = Some(sender);
    Ok(())
}

fn spawn_sync_worker(
    apply: impl Fn(bool) + Send + 'static,
) -> std::io::Result<mpsc::Sender<SyncRequest>> {
    let (sender, receiver) = mpsc::channel();
    std::thread::Builder::new()
        .name("auto-launch-sync".into())
        .spawn(move || run_sync_worker(receiver, apply))?;
    Ok(sender)
}

fn run_sync_worker(receiver: mpsc::Receiver<SyncRequest>, apply: impl Fn(bool)) {
    while let Ok(request) = receiver.recv() {
        let mut desired = request.desired;
        let mut completions: Vec<_> = request.completion.into_iter().collect();
        while let Ok(next) = receiver.try_recv() {
            desired = next.desired;
            completions.extend(next.completion);
        }

        apply(desired);
        for completion in completions {
            let _ = completion.send(());
        }
    }
}

fn sync_with(
    desired: bool,
    current_state: impl FnOnce() -> bool,
    update_state: impl FnOnce(bool) -> Result<()>,
) {
    if current_state() == desired {
        return;
    }
    if let Err(err) = update_state(desired) {
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
            cleanup_legacy_plist_at(&path);
        }
    }

    pub(super) fn cleanup_legacy_plist_at(path: &Path) {
        if path.exists() {
            if let Err(e) = fs::remove_file(path) {
                warn!(target: "auto_launch", "failed to remove legacy plist {}: {e}", path.display());
            } else {
                debug!(target: "auto_launch", "removed legacy LaunchAgent plist at {}", path.display());
            }
        }
    }

    pub(super) fn legacy_plist_path() -> Option<PathBuf> {
        let home = std::env::var("HOME").ok()?;
        Some(legacy_plist_path_under(Path::new(&home)))
    }

    pub(super) fn legacy_plist_path_under(home: &Path) -> PathBuf {
        home.join("Library")
            .join("LaunchAgents")
            .join(LEGACY_PLIST_NAME)
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

        #[test]
        fn legacy_plist_path_is_under_launch_agents() {
            let home = tempfile::tempdir().unwrap();
            let expected = home
                .path()
                .join("Library/LaunchAgents/com.bananatray.app.plist");

            let actual = super::super::platform::legacy_plist_path_under(home.path());

            assert_eq!(actual, expected);
        }

        #[test]
        fn cleanup_legacy_plist_removes_file() {
            let dir = tempfile::tempdir().unwrap();
            let plist = dir.path().join("com.bananatray.app.plist");
            fs::write(&plist, "<?xml version=\"1.0\"?><plist/>").unwrap();
            assert!(plist.exists());

            super::super::platform::cleanup_legacy_plist_at(&plist);

            assert!(!plist.exists());
        }

        #[test]
        fn cleanup_legacy_plist_noop_when_absent() {
            let dir = tempfile::tempdir().unwrap();
            let plist = dir.path().join("missing.plist");

            super::super::platform::cleanup_legacy_plist_at(&plist);

            assert!(!plist.exists());
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
    fn sync_skips_platform_update_when_state_matches() {
        let mut update_called = false;

        sync_with(
            false,
            || false,
            |_| {
                update_called = true;
                Ok(())
            },
        );

        assert!(!update_called);
    }

    #[test]
    fn sync_requests_platform_update_when_state_differs() {
        let mut requested_state = None;

        sync_with(
            false,
            || true,
            |desired| {
                requested_state = Some(desired);
                Ok(())
            },
        );

        assert_eq!(requested_state, Some(false));
    }

    #[test]
    fn sync_worker_serializes_updates_and_applies_latest_queued_state() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::{Arc, Barrier};

        let (sender, receiver) = mpsc::channel();
        let first_started = Arc::new(Barrier::new(2));
        let release_first = Arc::new(Barrier::new(2));
        let active = Arc::new(AtomicUsize::new(0));
        let max_active = Arc::new(AtomicUsize::new(0));
        let applied = Arc::new(Mutex::new(Vec::new()));

        let worker = {
            let first_started = first_started.clone();
            let release_first = release_first.clone();
            let active = active.clone();
            let max_active = max_active.clone();
            let applied = applied.clone();
            std::thread::spawn(move || {
                run_sync_worker(receiver, move |desired| {
                    let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                    max_active.fetch_max(current, Ordering::SeqCst);

                    let is_first = {
                        let mut states = applied.lock().unwrap();
                        states.push(desired);
                        states.len() == 1
                    };
                    if is_first {
                        first_started.wait();
                        release_first.wait();
                    }

                    active.fetch_sub(1, Ordering::SeqCst);
                });
            })
        };

        sender.send(SyncRequest::background(true)).unwrap();
        first_started.wait();
        sender.send(SyncRequest::background(false)).unwrap();
        sender.send(SyncRequest::background(true)).unwrap();
        sender.send(SyncRequest::background(false)).unwrap();
        release_first.wait();
        drop(sender);
        worker.join().unwrap();

        assert_eq!(*applied.lock().unwrap(), vec![true, false]);
        assert_eq!(max_active.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn sync_worker_acknowledges_waiter_only_after_latest_state_is_applied() {
        use std::sync::{Arc, Barrier};

        let (sender, receiver) = mpsc::channel();
        let (completion_tx, completion_rx) = mpsc::channel();
        let first_started = Arc::new(Barrier::new(2));
        let release_first = Arc::new(Barrier::new(2));
        let applied = Arc::new(Mutex::new(Vec::new()));

        let worker = {
            let first_started = first_started.clone();
            let release_first = release_first.clone();
            let applied = applied.clone();
            std::thread::spawn(move || {
                run_sync_worker(receiver, move |desired| {
                    let is_first = {
                        let mut states = applied.lock().unwrap();
                        states.push(desired);
                        states.len() == 1
                    };
                    if is_first {
                        first_started.wait();
                        release_first.wait();
                    }
                });
            })
        };

        sender.send(SyncRequest::background(true)).unwrap();
        first_started.wait();
        sender
            .send(SyncRequest::with_completion(false, completion_tx))
            .unwrap();

        assert_eq!(completion_rx.try_recv(), Err(mpsc::TryRecvError::Empty));

        release_first.wait();
        completion_rx.recv().unwrap();
        drop(sender);
        worker.join().unwrap();

        assert_eq!(*applied.lock().unwrap(), vec![true, false]);
    }
}
