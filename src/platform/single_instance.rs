//! Single-instance enforcement via `interprocess` Local Sockets.
//!
//! On first launch, binds a local socket and spawns a background listener thread.
//! On subsequent launches, connects to the existing socket, sends a "SHOW" command,
//! and exits.
//!
//! Platform mapping:
//! - **Windows**: Named Pipe (`\\.\pipe\bananatray`)
//! - **Linux**: Filesystem Unix Domain Socket in a per-user 0700 directory
//! - **macOS**: Filesystem Unix Domain Socket in `~/Library/Caches/bananatray/`
//!
//! # Threat model
//!
//! The listener accepts connections from any local process that can reach the socket,
//! so the IPC path is treated as untrusted input:
//!
//! - **Access control**: on Linux and macOS the socket lives inside a per-user directory
//!   (`0700`), so other users cannot connect at all. Linux used to bind an abstract-namespace
//!   socket, which has no filesystem node and therefore no permission gate — any local user
//!   could connect. That is why Linux now uses a filesystem socket.
//! - **Peer identity**: Unix connections are checked against the process's own uid (Linux
//!   `SO_PEERCRED` and the corresponding macOS peer-credential API via `peer_creds()`). Windows
//!   uses the named pipe's operating-system access control; its credential API exposes a peer PID,
//!   not a user ID.

use interprocess::local_socket::prelude::*;
#[cfg(not(windows))]
use interprocess::local_socket::GenericFilePath;
#[cfg(windows)]
use interprocess::local_socket::GenericNamespaced;
use interprocess::local_socket::{ListenerOptions, Name, Stream};
use log::{error, info, warn};
use std::io::{BufRead, BufReader, Write};
#[cfg(not(windows))]
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

#[cfg(unix)]
use std::fs::File;
#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

use super::APP_ID_LOWER;

const SHOW_CMD: &[u8] = b"SHOW\n";

/// Number of short retries before treating an old Unix socket as stale.
const PRIMARY_CONNECT_ATTEMPTS: usize = 3;

/// Outcome of the single-instance check.
#[allow(dead_code)]
pub enum InstanceRole {
    /// This is the first (primary) instance.
    /// The `mpsc::Receiver<()>` yields a value each time a secondary instance
    /// requests the primary to show its UI.
    Primary(mpsc::Receiver<()>),
    /// Another instance is already running. The "SHOW" command has been sent.
    Secondary,
}

#[cfg(windows)]
fn socket_name() -> Name<'static> {
    let name = single_instance_name();
    name.leak()
        .to_ns_name::<GenericNamespaced>()
        .expect("invalid socket name")
}

#[cfg(not(windows))]
fn socket_name() -> Name<'static> {
    socket_file_path()
        .into_os_string()
        .to_fs_name::<GenericFilePath>()
        .expect("invalid socket path")
        .into_owned()
}

/// Socket base name.
///
/// The socket lives in an app-owned subdirectory (see [`socket_dir`]), which already isolates
/// it per user, so no uid needs to be baked into the file name.
#[cfg(not(windows))]
fn single_instance_name() -> String {
    let base = match std::env::var("BANANATRAY_SINGLE_INSTANCE_SUFFIX") {
        Ok(suffix) if is_valid_instance_suffix(&suffix) => format!("{APP_ID_LOWER}-{suffix}"),
        Ok(suffix) => {
            warn!(
                target: "single_instance",
                "ignoring invalid BANANATRAY_SINGLE_INSTANCE_SUFFIX={suffix:?}"
            );
            APP_ID_LOWER.to_string()
        }
        Err(_) => APP_ID_LOWER.to_string(),
    };

    format!("{base}.sock")
}

#[cfg(windows)]
fn single_instance_name() -> String {
    match std::env::var("BANANATRAY_SINGLE_INSTANCE_SUFFIX") {
        Ok(suffix) if is_valid_instance_suffix(&suffix) => format!("{APP_ID_LOWER}-{suffix}"),
        Ok(suffix) => {
            warn!(
                target: "single_instance",
                "ignoring invalid BANANATRAY_SINGLE_INSTANCE_SUFFIX={suffix:?}"
            );
            APP_ID_LOWER.to_string()
        }
        Err(_) => APP_ID_LOWER.to_string(),
    }
}

fn is_valid_instance_suffix(suffix: &str) -> bool {
    !suffix.is_empty()
        && suffix
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

#[cfg(unix)]
fn current_uid() -> u32 {
    // SAFETY: `geteuid` is always safe to call; it takes no arguments and cannot fail.
    unsafe { libc::geteuid() }
}

/// Directory that holds the socket file on Unix.
///
/// This is an **app-owned subdirectory**, never a shared directory. That distinction matters:
/// [`ensure_private_dir`] enforces `0700` on whatever this returns, and it is only legitimate to
/// change the permissions of a directory the app owns. `$XDG_RUNTIME_DIR` and `~/Library/Caches`
/// belong to the whole user session, so the socket goes into a child directory instead.
///
/// Preference order:
/// 1. `$XDG_RUNTIME_DIR/{APP_ID_LOWER}` (Linux) — per-user tmpfs, cleaned on logout.
/// 2. `~/Library/Caches/{APP_ID_LOWER}` (macOS).
/// 3. The app config dir — used when the above are unavailable.
#[cfg(unix)]
fn socket_dir() -> PathBuf {
    #[cfg(target_os = "linux")]
    if let Some(runtime_dir) = std::env::var_os("XDG_RUNTIME_DIR") {
        let dir = PathBuf::from(runtime_dir);
        if dir.is_dir() {
            return dir.join(APP_ID_LOWER);
        }
    }

    #[cfg(target_os = "macos")]
    if let Some(cache_dir) = dirs::cache_dir() {
        return cache_dir.join(APP_ID_LOWER);
    }

    crate::platform::paths::app_config_dir()
}

#[cfg(unix)]
fn socket_file_path() -> PathBuf {
    socket_dir().join(single_instance_name())
}

#[cfg(unix)]
fn instance_lock_path() -> PathBuf {
    socket_file_path().with_extension("lock")
}

/// A process-lifetime advisory lock that distinguishes a live listener from a stale socket path.
#[cfg(unix)]
struct InstanceLock {
    _file: File,
}

#[cfg(unix)]
enum LockAcquireResult {
    Acquired(InstanceLock),
    Held,
    Unavailable(std::io::Error),
}

#[cfg(unix)]
fn acquire_instance_lock() -> LockAcquireResult {
    let path = instance_lock_path();
    let file = match std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .mode(0o600)
        .open(&path)
    {
        Ok(file) => file,
        Err(e) => return LockAcquireResult::Unavailable(e),
    };

    // SAFETY: flock only borrows the valid file descriptor and does not outlive `file`.
    let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if result == 0 {
        LockAcquireResult::Acquired(InstanceLock { _file: file })
    } else {
        let error = std::io::Error::last_os_error();
        match error.raw_os_error() {
            Some(code) if code == libc::EWOULDBLOCK || code == libc::EAGAIN => {
                LockAcquireResult::Held
            }
            _ => LockAcquireResult::Unavailable(error),
        }
    }
}

/// Ensure the app-owned socket directory exists and is only accessible by the current user
/// (`0700`).
///
/// Only ever call this with a directory the app owns (see [`socket_dir`]); tightening
/// permissions on a shared session directory would be overreach.
///
/// Failure is not fatal: the caller logs and continues without IPC rather than binding a socket
/// in a directory whose ownership or permissions were not verified.
#[cfg(unix)]
fn ensure_private_dir(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};

    if !dir.exists() {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)?;
    }

    let meta = std::fs::symlink_metadata(dir)?;
    if meta.file_type().is_symlink() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("socket directory {} is a symlink", dir.display()),
        ));
    }
    if !meta.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("socket directory {} is not a directory", dir.display()),
        ));
    }
    if meta.uid() != current_uid() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!(
                "socket directory {} is not owned by the current user",
                dir.display()
            ),
        ));
    }

    // 收紧权限：这是本应用自己的目录，其他用户不得进入（从而无法连接套接字）。
    let mode = meta.permissions().mode() & 0o777;
    if mode != 0o700 {
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Try to become the primary instance. If another instance is already running,
/// send a "SHOW" command and return `Secondary`.
#[allow(dead_code)]
pub fn ensure_single_instance() -> InstanceRole {
    #[cfg(unix)]
    {
        let dir = socket_dir();
        if let Err(e) = ensure_private_dir(&dir) {
            warn!(
                target: "single_instance",
                "failed to secure socket directory {} ({e}); disabling single-instance IPC",
                dir.display()
            );
            return empty_primary();
        }

        let lock = match acquire_instance_lock() {
            LockAcquireResult::Acquired(lock) => lock,
            LockAcquireResult::Held => {
                info!(target: "single_instance", "another instance holds the instance lock");
                if !notify_existing_instance() {
                    // The lock proves that another instance is alive. Never unlink its socket,
                    // even if a transient connection failure prevents SHOW from being delivered.
                    warn!(
                        target: "single_instance",
                        "failed to notify the locked primary instance; exiting as secondary"
                    );
                }
                return InstanceRole::Secondary;
            }
            LockAcquireResult::Unavailable(e) => {
                warn!(
                    target: "single_instance",
                    "failed to acquire instance lock ({e}); disabling single-instance IPC"
                );
                return empty_primary();
            }
        };

        // Try to create a listener (become primary). The lock makes stale-socket cleanup safe
        // for instances using this implementation, while the connect attempt preserves
        // compatibility with a primary started by an older build before the lock was added.
        let listener = ListenerOptions::new().name(socket_name()).create_sync();
        match listener {
            Ok(listener) => {
                info!(target: "single_instance", "primary instance: listener bound");
                InstanceRole::Primary(spawn_listener(listener, lock))
            }
            Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                if notify_existing_instance() {
                    return InstanceRole::Secondary;
                }

                warn!(
                    target: "single_instance",
                    "unreachable stale socket detected, cleaning up and becoming primary"
                );
                cleanup_stale_socket();
                match ListenerOptions::new().name(socket_name()).create_sync() {
                    Ok(listener) => {
                        info!(target: "single_instance", "became primary after cleanup");
                        InstanceRole::Primary(spawn_listener(listener, lock))
                    }
                    Err(e) => {
                        warn!(
                            target: "single_instance",
                            "failed to bind after cleanup ({e}); disabling single-instance IPC"
                        );
                        empty_primary()
                    }
                }
            }
            Err(e) => {
                warn!(
                    target: "single_instance",
                    "failed to bind listener ({e}); disabling single-instance IPC"
                );
                empty_primary()
            }
        }
    }

    #[cfg(windows)]
    {
        ensure_single_instance_windows()
    }
}

#[cfg(windows)]
fn ensure_single_instance_windows() -> InstanceRole {
    // Try to create a listener (become primary).
    let listener = ListenerOptions::new().name(socket_name()).create_sync();

    match listener {
        Ok(listener) => {
            info!(target: "single_instance", "primary instance: listener bound");
            InstanceRole::Primary(spawn_listener(listener, PrimaryGuard))
        }
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            info!(target: "single_instance", "another instance detected, sending SHOW command");
            if notify_existing_instance() {
                // Successfully sent SHOW command, another instance is running.
                InstanceRole::Secondary
            } else {
                // Named pipes do not leave stale filesystem nodes. A failed notification means
                // the existing pipe is unavailable or access-controlled; do not start a second
                // primary just because the SHOW handoff failed.
                warn!(
                    target: "single_instance",
                    "failed to notify the existing named-pipe instance; exiting as secondary"
                );
                InstanceRole::Secondary
            }
        }
        Err(e) => {
            // Unexpected error (e.g. permission denied). Log and proceed as primary
            // to avoid blocking the user from starting the app at all.
            warn!(target: "single_instance", "failed to bind listener ({e}), proceeding as primary");
            let (_tx, rx) = mpsc::channel();
            InstanceRole::Primary(rx)
        }
    }
}

fn empty_primary() -> InstanceRole {
    let (_tx, rx) = mpsc::channel();
    InstanceRole::Primary(rx)
}

#[cfg(unix)]
type PrimaryGuard = InstanceLock;

#[cfg(windows)]
struct PrimaryGuard;

/// 启动 listener 线程并返回 SHOW 通知的接收端。
///
/// 线程启动失败时降级继续运行（放弃二次实例 SHOW 中继），与 bind 失败的其他路径一致。
fn spawn_listener(
    listener: interprocess::local_socket::Listener,
    guard: PrimaryGuard,
) -> mpsc::Receiver<()> {
    let (tx, rx) = mpsc::channel();
    if let Err(e) = std::thread::Builder::new()
        .name("single-instance-listener".into())
        .spawn(move || accept_loop(listener, tx, guard))
    {
        warn!(target: "single_instance", "failed to spawn listener thread ({e}), proceeding without SHOW relay");
    }
    rx
}

fn accept_loop(
    listener: interprocess::local_socket::Listener,
    tx: mpsc::Sender<()>,
    _guard: PrimaryGuard,
) {
    for conn in listener.incoming() {
        match conn {
            Ok(stream) => handle_client(stream, &tx),
            Err(e) => {
                error!(target: "single_instance", "accept error: {e}");
            }
        }
    }
}

fn handle_client(stream: Stream, tx: &mpsc::Sender<()>) {
    let reader = BufReader::new(stream);
    for line in reader.lines() {
        match line {
            Ok(cmd) if cmd.trim() == "SHOW" => {
                info!(target: "single_instance", "received SHOW command from secondary instance");
                let _ = tx.send(());
            }
            Ok(other) => {
                warn!(target: "single_instance", "unknown command: {other}");
            }
            Err(e) => {
                error!(target: "single_instance", "read error: {e}");
                break;
            }
        }
    }
}

fn notify_existing_instance() -> bool {
    for attempt in 0..PRIMARY_CONNECT_ATTEMPTS {
        match Stream::connect(socket_name()) {
            Ok(mut stream) => {
                if let Err(e) = stream.write_all(SHOW_CMD) {
                    error!(target: "single_instance", "failed to send SHOW: {e}");
                } else {
                    return true;
                }
            }
            Err(e) => {
                error!(
                    target: "single_instance",
                    "failed to connect to primary instance (attempt {}): {e}",
                    attempt + 1
                );
            }
        }

        if attempt + 1 < PRIMARY_CONNECT_ATTEMPTS {
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    false
}

/// Clean up stale socket file from a dead instance.
///
/// On Unix, the socket is a filesystem node, so a crashed instance can leave it behind and
/// block the next bind with `AddrInUse`. On Windows, named pipes leave no files.
///
/// Note: `std::fs::remove_file` does NOT follow symlinks — it removes the symlink node itself,
/// so a symlink at this path cannot be used to delete arbitrary files.
#[cfg(unix)]
fn cleanup_stale_socket() {
    cleanup_stale_socket_path(&socket_file_path());
}

#[cfg(unix)]
fn cleanup_stale_socket_path(socket_path: &Path) {
    match std::fs::symlink_metadata(socket_path) {
        Ok(meta) if meta.file_type().is_dir() => {
            // 攻击者可能预先创建同名目录阻止 bind；尝试删除空目录
            if let Err(e) = std::fs::remove_dir(socket_path) {
                warn!(target: "single_instance", "stale path is a directory and could not be removed: {e}");
            }
        }
        Ok(_) => {
            if let Err(e) = std::fs::remove_file(socket_path) {
                warn!(target: "single_instance", "failed to remove stale socket file: {e}");
            }
        }
        Err(_) => {} // 不存在或无权访问，忽略
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_suffix_validation_accepts_safe_ascii() {
        assert!(is_valid_instance_suffix("gnome-dev"));
        assert!(is_valid_instance_suffix("dev_123"));
    }

    #[test]
    fn instance_suffix_validation_rejects_empty_or_path_like_values() {
        assert!(!is_valid_instance_suffix(""));
        assert!(!is_valid_instance_suffix("../real"));
        assert!(!is_valid_instance_suffix("with space"));
    }

    /// 在测试期间设置单实例后缀，并在 drop 时恢复原值。
    ///
    /// 环境变量是进程全局的，若不复原，后续测试会继承 "selftest" 后缀，
    /// 在并行测试下造成难以定位的干扰。持有互斥锁保证同一时刻只有一个
    /// 使用后缀的测试在跑。
    #[cfg(unix)]
    struct SuffixGuard {
        previous: Option<std::ffi::OsString>,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    #[cfg(unix)]
    impl SuffixGuard {
        fn set(suffix: &str) -> Self {
            static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
            // 毒化不影响正确性：这里保护的只是一个环境变量。
            let lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let previous = std::env::var_os("BANANATRAY_SINGLE_INSTANCE_SUFFIX");
            std::env::set_var("BANANATRAY_SINGLE_INSTANCE_SUFFIX", suffix);
            Self {
                previous,
                _lock: lock,
            }
        }
    }

    #[cfg(unix)]
    impl Drop for SuffixGuard {
        fn drop(&mut self) {
            match self.previous.take() {
                Some(previous) => std::env::set_var("BANANATRAY_SINGLE_INSTANCE_SUFFIX", previous),
                None => std::env::remove_var("BANANATRAY_SINGLE_INSTANCE_SUFFIX"),
            }
        }
    }

    /// 端到端验证：真实 bind + 真实连接 + SHOW 中继，且套接字落在 0700 的应用目录内。
    ///
    /// 用独立后缀隔离，避免与开发机上正在运行的真实实例争抢套接字。
    #[cfg(unix)]
    #[test]
    fn real_socket_round_trip_delivers_show_and_is_private() {
        use std::os::unix::fs::PermissionsExt;

        // 后缀必须与主进程隔离：测试进程与真实实例共用同一个 uid。
        let _guard = SuffixGuard::set("selftest");

        let path = socket_file_path();
        // 上一次失败可能留下陈旧节点。
        cleanup_stale_socket_path(&path);

        let listener = ListenerOptions::new().name(socket_name()).create_sync();
        let listener = match listener {
            Ok(l) => l,
            Err(e) => panic!("failed to bind test listener: {e}"),
        };

        let (tx, rx) = mpsc::channel();
        let handle = std::thread::spawn(move || {
            if let Some(Ok(stream)) = listener.incoming().next() {
                handle_client(stream, &tx);
            }
        });

        assert!(
            notify_existing_instance(),
            "secondary connect should succeed"
        );
        assert!(
            rx.recv_timeout(Duration::from_secs(5)).is_ok(),
            "primary should receive the SHOW notification"
        );

        handle.join().unwrap();
        cleanup_stale_socket_path(&path);

        // 套接字必须落在 0700 的应用自有目录中。
        let dir = socket_dir();
        ensure_private_dir(&dir).unwrap();
        let mode = std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700, "socket dir must be 0700, got {mode:o}");
    }

    #[cfg(unix)]
    #[test]
    fn socket_dir_is_private_after_ensure() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("runtime");

        ensure_private_dir(&dir).unwrap();

        let mode = std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
    }

    #[cfg(unix)]
    #[test]
    fn ensure_private_dir_tightens_loose_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("loose");
        std::fs::create_dir(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o777)).unwrap();

        ensure_private_dir(&dir).unwrap();

        let mode = std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
    }

    #[cfg(unix)]
    #[test]
    fn ensure_private_dir_rejects_symlink() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("target");
        std::fs::create_dir(&target).unwrap();
        let link = tmp.path().join("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        assert!(ensure_private_dir(&link).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn socket_file_path_lives_in_app_owned_subdir() {
        let path = socket_file_path();
        assert!(path.is_absolute(), "socket path must be absolute: {path:?}");

        let name = path.file_name().and_then(|s| s.to_str()).unwrap();
        assert!(name.starts_with(APP_ID_LOWER), "name: {name}");
        assert!(name.ends_with(".sock"), "name: {name}");

        // 必须位于本应用自己的子目录中：socket_dir() 会对其 chmod 0700，
        // 而只有自己的目录才可以改权限。
        let dir = path.parent().unwrap();
        assert_eq!(
            dir.file_name().and_then(|s| s.to_str()),
            Some(APP_ID_LOWER),
            "socket must live in an app-owned subdir, got: {dir:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn socket_dir_is_never_a_shared_session_directory() {
        // 防止回归：socket_dir() 必须返回本应用自己的子目录。
        // 早期实现直接返回 $XDG_RUNTIME_DIR / ~/Library/Caches，
        // 导致 ensure_private_dir 去 chmod 整个用户会话共享目录。
        let dir = socket_dir();
        assert_eq!(
            dir.file_name().and_then(|s| s.to_str()),
            Some(APP_ID_LOWER),
            "socket_dir must be app-owned, got: {dir:?}"
        );

        #[cfg(target_os = "linux")]
        if let Some(runtime_dir) = std::env::var_os("XDG_RUNTIME_DIR") {
            let runtime_dir = PathBuf::from(runtime_dir);
            if runtime_dir.is_dir() {
                assert_ne!(dir, runtime_dir, "must not be XDG_RUNTIME_DIR itself");
                assert!(
                    dir.starts_with(&runtime_dir),
                    "expected child of {runtime_dir:?}"
                );
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn cleanup_stale_socket_path_removes_stale_file() {
        let dir = tempfile::tempdir().unwrap();
        let socket_path = dir.path().join("single-instance.sock");
        std::fs::write(&socket_path, "stale").unwrap();

        cleanup_stale_socket_path(&socket_path);

        assert!(!socket_path.exists());
    }
}
