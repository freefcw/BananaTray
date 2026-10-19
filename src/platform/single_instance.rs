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
//! - **Read bounds**: each line is capped at [`MAX_COMMAND_LINE`] bytes and the read has a
//!   [`READ_TIMEOUT`] deadline, so a peer cannot grow the tray process's heap without bound
//!   or hold the accept loop open by never sending a newline.
//! - **Slow-client isolation**: each connection is served on its own thread, so one stuck
//!   client cannot starve the accept loop (which would otherwise break the SHOW relay and
//!   make secondary launches silently do nothing).

use interprocess::local_socket::prelude::*;
#[cfg(not(windows))]
use interprocess::local_socket::GenericFilePath;
#[cfg(windows)]
use interprocess::local_socket::GenericNamespaced;
use interprocess::local_socket::{ListenerOptions, Name, Stream};
use log::{error, info, warn};
use std::io::{BufRead, BufReader, Read, Write};
#[cfg(not(windows))]
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

#[cfg(unix)]
use std::fs::File;
#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

use super::APP_ID_LOWER;

const SHOW_CMD: &[u8] = b"SHOW\n";

/// Maximum accepted length of a single command line, in bytes.
///
/// The only legitimate payload is `"SHOW\n"` (5 bytes); this leaves generous slack while
/// keeping the worst-case allocation per connection trivially small.
const MAX_COMMAND_LINE: u64 = 64;

/// Deadline for a peer to deliver a complete command line.
///
/// A legitimate secondary instance writes `SHOW\n` immediately, so this only ever fires for
/// a stalled or hostile peer.
const READ_TIMEOUT: Duration = Duration::from_secs(5);

/// Maximum number of connection threads that may be active at once.
const MAX_ACTIVE_CONNECTIONS: usize = 32;

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
    let active_connections = Arc::new(AtomicUsize::new(0));
    for conn in listener.incoming() {
        match conn {
            Ok(stream) => {
                if !try_acquire_connection(&active_connections) {
                    warn!(
                        target: "single_instance",
                        "too many active single-instance connections; rejecting peer"
                    );
                    continue;
                }

                // 每个连接独立线程处理：恶意/卡死的客户端不能阻塞 accept，
                // 否则二次启动的 SHOW 中继会失效。
                let tx = tx.clone();
                let active_connections = Arc::clone(&active_connections);
                let permit = ConnectionPermit(active_connections);
                if let Err(e) = std::thread::Builder::new()
                    .name("single-instance-conn".into())
                    .spawn(move || {
                        let _permit = permit;
                        handle_client(stream, &tx);
                    })
                {
                    warn!(target: "single_instance", "failed to spawn connection thread ({e})");
                }
            }
            Err(e) => {
                error!(target: "single_instance", "accept error: {e}");
            }
        }
    }
}

fn try_acquire_connection(active_connections: &AtomicUsize) -> bool {
    active_connections
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
            (current < MAX_ACTIVE_CONNECTIONS).then_some(current + 1)
        })
        .is_ok()
}

struct ConnectionPermit(Arc<AtomicUsize>);

impl Drop for ConnectionPermit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

fn handle_client(stream: Stream, tx: &mpsc::Sender<()>) {
    // Unix 只接受本进程所属用户的连接；目录权限是额外的纵深防御。
    if !peer_is_same_user(&stream) {
        warn!(target: "single_instance", "rejected connection from a different user");
        return;
    }

    // 读取超时：合法二次实例会立即写入 "SHOW\n"，超时只对卡死/恶意对端生效。
    // Windows 命名管道不支持 I/O 超时（返回 Unsupported），此时依赖长度上限兜底。
    if let Err(e) = stream.set_recv_timeout(Some(READ_TIMEOUT)) {
        if e.kind() != std::io::ErrorKind::Unsupported {
            warn!(target: "single_instance", "failed to set read timeout ({e})");
        }
    }

    // 长度上限：`lines()` 会把整行累积进进程内 String，对端不发换行即可无限撑大堆。
    // `take` 在超过上限时截断，`read_line` 返回后按长度判定为非法输入并断开。
    let mut reader = BufReader::new(stream.take(MAX_COMMAND_LINE + 1));
    let mut buf = String::new();
    match reader.read_line(&mut buf) {
        Ok(0) => {} // 对端关闭
        Ok(_) if buf.len() as u64 > MAX_COMMAND_LINE => {
            warn!(
                target: "single_instance",
                "rejected oversized command line (>{MAX_COMMAND_LINE} bytes)"
            );
        }
        Ok(_) if buf.trim() == "SHOW" => {
            info!(target: "single_instance", "received SHOW command from secondary instance");
            let _ = tx.send(());
        }
        Ok(_) => {
            warn!(target: "single_instance", "unknown command: {}", buf.trim());
        }
        Err(e) if is_timeout_error(&e) => {
            warn!(target: "single_instance", "read timed out, closing connection");
        }
        Err(e) => {
            error!(target: "single_instance", "read error: {e}");
        }
    }
}

/// Whether an I/O error represents a read timeout rather than a real failure.
fn is_timeout_error(e: &std::io::Error) -> bool {
    matches!(
        e.kind(),
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
    )
}

/// Verify the Unix peer runs as the same user as this process.
///
/// Returns `true` only when the platform reports a matching uid or does not expose a uid. A
/// credential-query error is rejected because it may indicate that the peer cannot be
/// authenticated reliably.
#[cfg(unix)]
fn peer_is_same_user(stream: &Stream) -> bool {
    match stream.peer_creds() {
        Ok(creds) => match creds.euid() {
            Some(peer_uid) => peer_uid == current_uid(),
            // 平台不提供 euid（如部分 BSD）：放行，依赖目录权限。
            None => true,
        },
        Err(e) => {
            warn!(target: "single_instance", "failed to read peer credentials ({e}), rejecting connection");
            false
        }
    }
}

#[cfg(windows)]
fn peer_is_same_user(_stream: &Stream) -> bool {
    // Windows named-pipe access is controlled by the pipe's OS security descriptor. The
    // interprocess peer-credential API exposes a PID on Windows, not a comparable user ID.
    true
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

    #[test]
    fn oversized_line_is_truncated_at_the_cap() {
        // 模拟恶意对端：不发送换行，持续写入远超上限的数据。
        let payload = vec![b'A'; (MAX_COMMAND_LINE as usize) * 8];
        let mut reader = BufReader::new(std::io::Cursor::new(payload).take(MAX_COMMAND_LINE + 1));
        let mut buf = String::new();
        reader.read_line(&mut buf).unwrap();

        // take() 保证不会读到超过上限，因此堆分配有界。
        assert!(buf.len() as u64 <= MAX_COMMAND_LINE + 1);
        assert!(
            buf.len() as u64 > MAX_COMMAND_LINE,
            "should be detected as oversized"
        );
    }

    #[test]
    fn normal_show_command_fits_within_the_cap() {
        assert!(SHOW_CMD.len() as u64 <= MAX_COMMAND_LINE);
        assert_eq!(String::from_utf8_lossy(SHOW_CMD).trim(), "SHOW");
    }

    #[test]
    fn active_connection_limit_releases_permits_after_disconnect() {
        let active_connections = Arc::new(AtomicUsize::new(0));
        let mut permits = Vec::new();

        for _ in 0..MAX_ACTIVE_CONNECTIONS {
            assert!(try_acquire_connection(&active_connections));
            permits.push(ConnectionPermit(Arc::clone(&active_connections)));
        }
        assert!(!try_acquire_connection(&active_connections));

        permits.pop();
        assert!(try_acquire_connection(&active_connections));
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

    /// 恶意对端不发换行、持续写入时，必须在长度上限处被断开。
    ///
    /// 关键断言：连接在**写入极少数据后**就被服务端关闭，而不是等读到海量数据。
    /// 若长度上限缺失（退回无界 `lines()`），服务端会持续收数据，写入端不会很快失败。
    #[cfg(unix)]
    #[test]
    fn real_socket_rejects_endless_line_without_exhausting_memory() {
        use std::io::Write as _;

        let _guard = SuffixGuard::set("selftest-dos");

        let path = socket_file_path();
        cleanup_stale_socket_path(&path);

        let listener = ListenerOptions::new()
            .name(socket_name())
            .create_sync()
            .expect("failed to bind test listener");

        let (tx, rx) = mpsc::channel();
        let handle = std::thread::spawn(move || {
            if let Some(Ok(stream)) = listener.incoming().next() {
                handle_client(stream, &tx);
            }
        });

        // 模拟攻击：只写数据，永不发送换行。
        let mut attacker = Stream::connect(socket_name()).expect("attacker connect");
        let chunk = vec![b'A'; 4096];
        let mut bytes_written = 0usize;
        for _ in 0..256 {
            match attacker.write_all(&chunk) {
                Ok(()) => bytes_written += chunk.len(),
                // 对端被断开后写入失败 —— 这正是期望结果。
                Err(_) => break,
            }
            // 服务端一旦按上限断开，写入很快会失败；无需写满全部循环。
            if bytes_written > 64 * 1024 {
                break;
            }
        }

        // 攻击者必须被断开，且没有 SHOW 被中继。
        assert!(
            !rx.recv_timeout(Duration::from_secs(2)).is_ok(),
            "oversized line must not be treated as SHOW"
        );

        // 关键：服务端必须在读到极少量数据后就断开（上限 64 字节），
        // 而不是把对端发来的所有内容都收下。
        assert!(
            bytes_written <= 64 * 1024,
            "server accepted {bytes_written} bytes; length cap is not enforced"
        );

        handle.join().unwrap();
        cleanup_stale_socket_path(&path);
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
