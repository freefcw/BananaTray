//! Single-instance enforcement via `interprocess` Local Sockets.
//!
//! On first launch, binds a local socket and spawns a background listener thread.
//! On subsequent launches, connects to the existing socket, sends a "SHOW" command,
//! and exits.
//!
//! Platform mapping (handled by `interprocess::local_socket::GenericNamespaced`):
//! - **Windows**: Named Pipe (`\\.\pipe\bananatray`)
//! - **Linux**: Abstract Unix Domain Socket
//! - **macOS**: Unix Domain Socket at `/tmp/bananatray`

use interprocess::local_socket::prelude::*;
use interprocess::local_socket::{GenericNamespaced, ListenerOptions, Name, Stream};
use log::{error, info, warn};
use std::io::{BufRead, BufReader, Write};
use std::sync::mpsc;

const SOCKET_NAME: &str = "bananatray";
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

fn socket_name() -> Name<'static> {
    SOCKET_NAME
        .to_ns_name::<GenericNamespaced>()
        .expect("invalid socket name")
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
            notify_existing_instance();
            InstanceRole::Secondary
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

fn notify_existing_instance() {
    let name = socket_name();
    match Stream::connect(name) {
        Ok(mut stream) => {
            if let Err(e) = stream.write_all(SHOW_CMD) {
                error!(target: "single_instance", "failed to send SHOW: {e}");
            }
        }
        Err(e) => {
            error!(target: "single_instance", "failed to connect to primary instance: {e}");
        }
    }
}
