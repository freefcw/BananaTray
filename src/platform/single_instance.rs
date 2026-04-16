//! Single-instance enforcement via `interprocess` Local Sockets.
//!
//! On first launch, binds a local socket and spawns a background listener thread.
//! On subsequent launches, connects to the existing socket, sends a "SHOW" command,
//! and exits.
//!
//! Platform mapping:
//! - **Windows**: Named Pipe (`\\.\pipe\bananatray`)
//! - **Linux**: Abstract Unix Domain Socket
//! - **macOS**: Per-user Unix Domain Socket at `~/Library/Caches/bananatray.sock`

use interprocess::local_socket::prelude::*;
#[cfg(target_os = "macos")]
use interprocess::local_socket::GenericFilePath;
#[cfg(not(target_os = "macos"))]
use interprocess::local_socket::GenericNamespaced;
use interprocess::local_socket::{ListenerOptions, Name, Stream};
use log::{error, info, warn};
use std::io::{BufRead, BufReader, Write};
#[cfg(target_os = "macos")]
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use super::APP_ID_LOWER;

const SOCKET_NAME: &str = APP_ID_LOWER;
const SHOW_CMD: &[u8] = b"SHOW\n";

/// Outcome of the single-instance check.
pub enum InstanceRole {
    /// This is the first (primary) instance.
    /// The `mpsc::Receiver<()>` yields a value each time a secondary instance
    /// requests the primary to show its UI.
    Primary(mpsc::Receiver<()>),
    /// Another instance is already running. The "SHOW" command has been sent.
    Secondary,
}

#[cfg(not(target_os = "macos"))]
fn socket_name() -> Name<'static> {
    SOCKET_NAME
        .to_ns_name::<GenericNamespaced>()
        .expect("invalid socket name")
}

#[cfg(target_os = "macos")]
fn socket_name() -> Name<'static> {
    socket_file_path()
        .into_os_string()
        .to_fs_name::<GenericFilePath>()
        .expect("invalid socket path")
        .into_owned()
}

#[cfg(target_os = "macos")]
fn socket_file_path() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(crate::platform::paths::app_config_dir)
        .join(format!("{APP_ID_LOWER}.sock"))
}

/// Try to become the primary instance. If another instance is already running,
/// send a "SHOW" command and return `Secondary`.
pub fn ensure_single_instance() -> InstanceRole {
    // Try to create a listener (become primary).
    let listener = ListenerOptions::new().name(socket_name()).create_sync();

    match listener {
        Ok(listener) => {
            info!(target: "single_instance", "primary instance: listener bound");
            let (tx, rx) = mpsc::channel();
            std::thread::Builder::new()
                .name("single-instance-listener".into())
                .spawn(move || accept_loop(listener, tx))
                .expect("failed to spawn single-instance listener thread");
            InstanceRole::Primary(rx)
        }
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            info!(target: "single_instance", "another instance detected, sending SHOW command");
            if notify_existing_instance() {
                // Successfully sent SHOW command, another instance is running.
                InstanceRole::Secondary
            } else {
                // Failed to connect - stale socket file from dead instance.
                // Clean up and become primary.
                warn!(target: "single_instance", "stale socket detected, cleaning up and becoming primary");
                cleanup_stale_socket();
                // Retry becoming primary
                let listener = ListenerOptions::new().name(socket_name()).create_sync();
                match listener {
                    Ok(listener) => {
                        info!(target: "single_instance", "became primary after cleanup");
                        let (tx, rx) = mpsc::channel();
                        std::thread::Builder::new()
                            .name("single-instance-listener".into())
                            .spawn(move || accept_loop(listener, tx))
                            .expect("failed to spawn single-instance listener thread");
                        InstanceRole::Primary(rx)
                    }
                    Err(e) => {
                        // Unexpected error after cleanup - proceed anyway
                        warn!(target: "single_instance", "failed to bind after cleanup ({e}), proceeding as primary");
                        let (_tx, rx) = mpsc::channel();
                        InstanceRole::Primary(rx)
                    }
                }
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

fn accept_loop(listener: interprocess::local_socket::Listener, tx: mpsc::Sender<()>) {
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
    let name = socket_name();
    match Stream::connect(name) {
        Ok(mut stream) => {
            if let Err(e) = stream.write_all(SHOW_CMD) {
                error!(target: "single_instance", "failed to send SHOW: {e}");
                false
            } else {
                true
            }
        }
        Err(e) => {
            error!(target: "single_instance", "failed to connect to primary instance: {e}");
            false
        }
    }
}

/// Clean up stale socket file from a dead instance.
/// On macOS, the socket file is under `~/Library/Caches/{APP_ID_LOWER}.sock`.
/// On Linux, abstract sockets don't leave files, so no cleanup needed.
/// On Windows, named pipes don't leave files either.
///
/// Note: `std::fs::remove_file` does NOT follow symlinks — it removes the symlink node itself,
/// so a symlink at this path cannot be used to delete arbitrary files.
#[cfg(target_os = "macos")]
fn cleanup_stale_socket() {
    cleanup_stale_socket_path(&socket_file_path());
}

#[cfg(target_os = "macos")]
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

#[cfg(not(target_os = "macos"))]
fn cleanup_stale_socket() {
    // On Linux (abstract sockets) and Windows (named pipes),
    // there are no persistent files to clean up.
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "macos")]
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn socket_file_path_uses_user_cache_dir() {
        let path = socket_file_path();
        assert_eq!(
            path.file_name().and_then(|s| s.to_str()),
            Some("bananatray.sock")
        );
        assert!(path.to_string_lossy().contains("/Library/Caches/"));
        assert!(path.to_string_lossy().contains(APP_ID_LOWER));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn cleanup_stale_socket_path_removes_stale_file() {
        let dir = tempfile::tempdir().unwrap();
        let socket_path = dir.path().join("single-instance.sock");
        std::fs::write(&socket_path, "stale").unwrap();

        cleanup_stale_socket_path(&socket_path);

        assert!(!socket_path.exists());
    }
}
