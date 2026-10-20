//! Interactive PTY runner for CLI commands that may require user interaction.
//!
//! Many CLI tools (like Claude, Codex) detect when they're not running in a
//! real terminal and may show interactive prompts. This runner simulates a
//! terminal session so these tools produce their normal output, while also
//! automatically responding to known prompts.

use anyhow::Result;
use portable_pty::{native_pty_system, CommandBuilder, PtyPair, PtySize, PtySystem};
use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::providers::common::path_resolver;
use crate::utils::text_utils;

const READER_CHUNK_SIZE: usize = 8192;
/// Reader queue capacity. A bounded queue applies backpressure before the
/// captured buffer reaches its hard limit.
const READER_QUEUE_CAPACITY: usize = 64;
const CAPTURE_POLL_INTERVAL: Duration = Duration::from_millis(60);
/// PTY 捕获缓冲上限。
///
/// 捕获循环每收到一块就把整个缓冲重新转成字符串做提示匹配，缓冲无上限时
/// 一条狂吐输出的 CLI 会在超时前把内存冲上去。
const MAX_CAPTURE_BYTES: usize = super::MAX_EXTERNAL_PAYLOAD_BYTES as usize;
/// How long to wait for a killed child to be reaped before giving up.
const REAP_TIMEOUT: Duration = Duration::from_secs(2);
const REAP_POLL_INTERVAL: Duration = Duration::from_millis(20);

/// Result of running an interactive command
#[derive(Debug)]
pub struct InteractiveResult {
    /// The captured output from the command
    pub output: String,
}

/// Options for running an interactive command
#[derive(Debug, Clone)]
pub struct InteractiveOptions {
    /// Maximum time to wait for the command to complete
    pub timeout: Duration,
    /// Time to wait without new meaningful data before considering done
    pub idle_timeout: Duration,
    /// Directory to run the command in
    pub working_directory: Option<std::path::PathBuf>,
    /// Arguments to pass to the command
    pub arguments: Vec<String>,
    /// Automatic responses to prompts. Maps prompt text to the response to send.
    /// Example: `["Continue? [y/n]": "y\n"]` will auto-respond "y" when prompted.
    pub auto_responses: HashMap<String, String>,
    /// Environment variable keys to exclude from the subprocess environment
    pub environment_exclusions: Vec<String>,
    /// Send periodic Enter key to keep output flowing (useful for some CLIs)
    pub send_enter_every: Option<Duration>,
    /// Time to wait after spawning before sending input (process init delay)
    pub init_delay: Duration,
}

impl Default for InteractiveOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(20),
            idle_timeout: Duration::from_secs(3),
            working_directory: None,
            arguments: Vec::new(),
            auto_responses: HashMap::new(),
            environment_exclusions: Vec::new(),
            send_enter_every: None,
            init_delay: Duration::from_millis(400),
        }
    }
}

/// Errors that can occur when running an interactive command
#[derive(Debug, Clone)]
pub enum InteractiveError {
    /// CLI tool not found
    BinaryNotFound(String),
    /// Failed to create PTY
    PtyFailed(String),
    /// Failed to start command
    LaunchFailed(String),
}

impl std::fmt::Display for InteractiveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BinaryNotFound(cli) => {
                write!(
                    f,
                    "CLI '{}' not found. Please install it and ensure it's on PATH.",
                    cli
                )
            }
            Self::PtyFailed(msg) => write!(f, "Failed to create terminal session: {}", msg),
            Self::LaunchFailed(msg) => write!(f, "Failed to start command: {}", msg),
        }
    }
}

impl std::error::Error for InteractiveError {}

impl From<InteractiveError> for crate::providers::ProviderError {
    fn from(err: InteractiveError) -> Self {
        match err {
            InteractiveError::BinaryNotFound(cli) => Self::cli_not_found(&cli),
            other => Self::fetch_failed(&other.to_string()),
        }
    }
}

/// Runner for interactive CLI commands using a pseudo-terminal (PTY)
pub struct InteractiveRunner {
    pty_system: Box<dyn PtySystem>,
}

impl Default for InteractiveRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl InteractiveRunner {
    /// Create a new interactive runner
    pub fn new() -> Self {
        Self {
            pty_system: native_pty_system(),
        }
    }

    /// Run a command and capture its output, automatically responding to prompts.
    ///
    /// # Arguments
    /// * `binary` - The CLI tool to run (e.g., "claude", "codex")
    /// * `input` - Text to send to the command (e.g., "/usage")
    /// * `options` - Configuration for timeout, arguments, and auto-responses
    ///
    /// # Returns
    /// The captured output
    pub fn run(
        &self,
        binary: &str,
        input: &str,
        options: InteractiveOptions,
    ) -> Result<InteractiveResult> {
        let start = Instant::now();

        // Find executable
        let executable_path = path_resolver::locate_executable(binary)
            .ok_or_else(|| InteractiveError::BinaryNotFound(binary.to_string()))?;
        log::info!(
            target: "interactive_runner",
            "[{}] Found executable at '{}' ({:.0}ms)",
            binary, executable_path, start.elapsed().as_millis()
        );

        // Create PTY
        let pair = self.create_pty()?;

        // Spawn process. From here on the child is owned by a guard that attempts
        // to terminate and reap it on every exit path.
        let mut child = ChildGuard::new(self.spawn_process(&pair, &executable_path, &options)?);
        log::info!(
            target: "interactive_runner",
            "[{}] Process spawned ({:.0}ms), waiting {:.0}ms init delay",
            binary, start.elapsed().as_millis(), options.init_delay.as_millis()
        );

        // Allow process to initialize
        std::thread::sleep(options.init_delay);

        #[cfg(unix)]
        child.set_process_group(pair.master.process_group_leader());

        // Take the writer once and hold it for the whole session.
        // `take_writer` is one-shot, so taking it per-write made every
        // auto-response fail with "cannot take writer more than once".
        // Holding it also avoids the premature EOF that dropping it sends to
        // the slave (UnixMasterWriter::drop writes a newline + EOT).
        let mut writer = pair.master.take_writer()?;

        // Send input command
        if !input.trim().is_empty() {
            let input_data = format!("{}\r", input.trim());
            log::info!(
                target: "interactive_runner",
                "[{}] Sending input ({} bytes): {:?} ({:.0}ms)",
                binary, input_data.len(), input_data, start.elapsed().as_millis()
            );
            writer.write_all(input_data.as_bytes())?;
        }

        // Capture output with auto-response handling
        let buffer = self.capture_output(&pair, &mut child, &mut writer, &options)?;

        let elapsed = start.elapsed();
        log::debug!(
            target: "interactive_runner",
            "Command '{}' completed in {:.3}s, output length: {} bytes",
            binary,
            elapsed.as_secs_f64(),
            buffer.len()
        );

        // Strip ANSI codes from output
        let output = text_utils::strip_ansi(&String::from_utf8_lossy(&buffer));

        Ok(InteractiveResult { output })
    }

    /// Create a pseudo-terminal
    fn create_pty(&self) -> Result<PtyPair> {
        let size = PtySize {
            rows: 50,
            cols: 160,
            pixel_width: 0,
            pixel_height: 0,
        };

        self.pty_system
            .openpty(size)
            .map_err(|e| InteractiveError::PtyFailed(e.to_string()).into())
    }

    /// Spawn the process with the given options
    fn spawn_process(
        &self,
        pair: &PtyPair,
        executable_path: &str,
        options: &InteractiveOptions,
    ) -> Result<Box<dyn portable_pty::Child + Send + Sync>> {
        let mut cmd = CommandBuilder::new(executable_path);
        cmd.args(&options.arguments);

        // Set working directory
        if let Some(ref dir) = options.working_directory {
            cmd.cwd(dir);
        }

        // Set up environment
        let mut env: HashMap<String, String> = std::env::vars().collect();

        // Remove excluded keys
        for key in &options.environment_exclusions {
            env.remove(key);
        }

        // Ensure common paths are included in PATH.
        let path = env.get("PATH").map(String::as_str).unwrap_or_default();
        env.insert("PATH".to_string(), path_resolver::enrich_path(path));

        // Set terminal environment
        env.entry("TERM".to_string())
            .or_insert("xterm-256color".to_string());
        env.entry("COLORTERM".to_string())
            .or_insert("truecolor".to_string());
        env.entry("LANG".to_string())
            .or_insert("en_US.UTF-8".to_string());

        // Apply environment variables one by one
        for (key, value) in &env {
            cmd.env(key, value);
        }

        // Spawn using the slave side of PTY
        pair.slave
            .spawn_command(cmd)
            .map_err(|e| InteractiveError::LaunchFailed(e.to_string()).into())
    }

    /// Capture output from the PTY, automatically responding to prompts.
    ///
    /// Uses a dedicated reader thread to avoid blocking on PTY read, which would
    /// prevent timeout and idle checks from running.
    fn capture_output(
        &self,
        pair: &PtyPair,
        child: &mut ChildGuard,
        writer: &mut Box<dyn Write + Send>,
        options: &InteractiveOptions,
    ) -> Result<Vec<u8>> {
        let mut reader = spawn_reader(pair)?;
        let receiver = reader.take_receiver();
        let result = CaptureLoop::new(child, writer, options, receiver).run();

        // Stop/reap the process before joining the reader. On Unix this also
        // closes the PTY slave held by descendants, while the reader's poll
        // loop provides a bounded escape even if the child cleanup fails.
        child.reap();
        reader.stop_and_join();

        result
    }

    fn normalize_for_matching(text: &str) -> String {
        text_utils::normalize_for_matching(text)
    }
}

/// Owns the spawned child and attempts to terminate and reap it on every exit
/// path, including `?` early returns.
///
/// `portable_pty` wraps `std::process::Child`, which neither kills nor waits on
/// drop, so without this guard any error between spawn and the explicit cleanup
/// leaves the process unreaped.
struct ChildGuard {
    child: Box<dyn portable_pty::Child + Send + Sync>,
    #[cfg(unix)]
    process_group_id: Option<libc::pid_t>,
    cleanup_attempted: bool,
}

impl ChildGuard {
    fn new(child: Box<dyn portable_pty::Child + Send + Sync>) -> Self {
        Self {
            child,
            #[cfg(unix)]
            process_group_id: None,
            cleanup_attempted: false,
        }
    }

    #[cfg(unix)]
    fn set_process_group(&mut self, process_group_id: Option<libc::pid_t>) {
        self.process_group_id = process_group_id;
    }

    fn child_mut(&mut self) -> &mut dyn portable_pty::Child {
        self.child.as_mut()
    }

    /// Attempts to terminate the process tree, then waits for the direct child
    /// to be reaped. Cleanup is bounded so a broken child implementation cannot
    /// block the caller forever.
    fn reap(&mut self) {
        self.reap_with_timeout(REAP_TIMEOUT);
    }

    fn reap_with_timeout(&mut self, timeout: Duration) {
        if self.cleanup_attempted {
            return;
        }
        self.cleanup_attempted = true;

        match self.child.try_wait() {
            Ok(Some(_)) => {
                // The direct child is already gone, but descendants may still
                // hold the PTY slave open.
                #[cfg(unix)]
                self.kill_process_group();
                return;
            }
            Ok(None) => {}
            Err(e) => {
                log::warn!(
                    target: "interactive_runner",
                    "try_wait failed before child cleanup: {}; attempting termination",
                    e
                );
            }
        }

        if let Err(e) = self.child.kill() {
            if e.kind() != std::io::ErrorKind::NotFound {
                log::warn!(
                    target: "interactive_runner",
                    "failed to terminate interactive child: {}",
                    e
                );
            }
        }

        #[cfg(unix)]
        self.kill_process_group();

        let deadline = Instant::now() + timeout;
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(REAP_POLL_INTERVAL);
                }
                Ok(None) => {
                    log::warn!(
                        target: "interactive_runner",
                        "Child did not exit within {:?} after termination; leaving it unreaped",
                        timeout
                    );
                    return;
                }
                Err(e) => {
                    log::warn!(
                        target: "interactive_runner",
                        "try_wait failed while reaping child: {}",
                        e
                    );
                    return;
                }
            }
        }
    }

    #[cfg(unix)]
    fn kill_process_group(&self) {
        let Some(process_group_id) = self.process_group_id else {
            return;
        };
        if process_group_id <= 1 {
            return;
        }

        let result = unsafe { libc::kill(-process_group_id, libc::SIGKILL) };
        if result != 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::ESRCH) {
                log::warn!(
                    target: "interactive_runner",
                    "failed to terminate PTY process group {}: {}",
                    process_group_id,
                    error
                );
            }
        }
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        self.reap();
    }
}

struct ReaderThread {
    receiver: Option<Receiver<Vec<u8>>>,
    stop: Arc<AtomicBool>,
    handle: JoinHandle<()>,
}

impl ReaderThread {
    fn take_receiver(&mut self) -> Receiver<Vec<u8>> {
        self.receiver
            .take()
            .expect("PTY reader receiver can only be taken once")
    }

    fn stop_and_join(self) {
        self.stop.store(true, Ordering::Relaxed);
        if self.handle.join().is_err() {
            log::warn!(target: "interactive_runner", "PTY reader thread panicked");
        }
    }
}

fn spawn_reader(pair: &PtyPair) -> Result<ReaderThread> {
    let mut reader = pair.master.try_clone_reader()?;
    let stop = Arc::new(AtomicBool::new(false));
    let reader_stop = Arc::clone(&stop);
    // `channel()` is intentionally not used here: the capture loop consumes
    // at most one chunk per poll, so an unbounded queue could grow without
    // limit while a CLI is producing output faster than we can inspect it.
    let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(READER_QUEUE_CAPACITY);

    #[cfg(unix)]
    let raw_fd = pair.master.as_raw_fd();

    let handle = std::thread::spawn(move || {
        let mut chunk = [0u8; READER_CHUNK_SIZE];
        loop {
            if reader_stop.load(Ordering::Relaxed) {
                break;
            }

            #[cfg(unix)]
            {
                let Some(raw_fd) = raw_fd else {
                    break;
                };
                let mut poll_fd = libc::pollfd {
                    fd: raw_fd,
                    events: libc::POLLIN | libc::POLLHUP | libc::POLLERR,
                    revents: 0,
                };
                let poll_result = unsafe { libc::poll(&mut poll_fd, 1, 100) };
                if poll_result < 0 {
                    if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                        continue;
                    }
                    break;
                }
                if poll_result == 0 {
                    continue;
                }
            }

            match reader.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    if tx.send(chunk[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    Ok(ReaderThread {
        receiver: Some(rx),
        stop,
        handle,
    })
}

struct PromptResponse {
    normalized_prompt: String,
    response: String,
}

struct PromptMatch {
    normalized_prompt: String,
    response: String,
}

struct PromptResponder {
    prompt_responses: Vec<PromptResponse>,
    responded_prompts: HashSet<String>,
}

impl PromptResponder {
    fn new(auto_responses: &HashMap<String, String>) -> Self {
        let prompt_responses = auto_responses
            .iter()
            .map(|(prompt, response)| PromptResponse {
                normalized_prompt: InteractiveRunner::normalize_for_matching(prompt),
                response: response.clone(),
            })
            .collect();

        Self {
            prompt_responses,
            responded_prompts: HashSet::new(),
        }
    }

    fn take_matches(&mut self, output: &str) -> Vec<PromptMatch> {
        let normalized_output = InteractiveRunner::normalize_for_matching(output);
        let mut matches = Vec::new();

        for prompt_response in &self.prompt_responses {
            if !self
                .responded_prompts
                .contains(&prompt_response.normalized_prompt)
                && normalized_output.contains(&prompt_response.normalized_prompt)
            {
                self.responded_prompts
                    .insert(prompt_response.normalized_prompt.clone());
                matches.push(PromptMatch {
                    normalized_prompt: prompt_response.normalized_prompt.clone(),
                    response: prompt_response.response.clone(),
                });
            }
        }

        matches
    }
}

struct CaptureState {
    buffer: Vec<u8>,
    deadline: Instant,
    last_meaningful_data: Instant,
    last_enter: Instant,
}

impl CaptureState {
    fn new(timeout: Duration, now: Instant) -> Self {
        Self {
            buffer: Vec::new(),
            deadline: now + timeout,
            last_meaningful_data: now,
            last_enter: now,
        }
    }

    fn should_continue(&self, now: Instant) -> bool {
        now < self.deadline
    }

    fn timed_out(&self, now: Instant) -> bool {
        now >= self.deadline
    }

    /// 追加一块输出；达到上限后截断，由 `is_full` 通知捕获循环停止。
    fn record_chunk(&mut self, data: &[u8], now: Instant) {
        if text_utils::has_meaningful_content(data) {
            self.last_meaningful_data = now;
        }
        let remaining = MAX_CAPTURE_BYTES.saturating_sub(self.buffer.len());
        self.buffer
            .extend_from_slice(&data[..data.len().min(remaining)]);
    }

    fn is_full(&self) -> bool {
        self.buffer.len() >= MAX_CAPTURE_BYTES
    }

    fn record_prompt_response(&mut self, now: Instant) {
        self.last_meaningful_data = now;
    }

    fn is_idle(&self, idle_timeout: Duration, now: Instant) -> bool {
        !self.buffer.is_empty() && now.duration_since(self.last_meaningful_data) > idle_timeout
    }

    fn should_send_enter(&self, every: Duration, now: Instant) -> bool {
        now.duration_since(self.last_enter) >= every
    }

    fn record_enter_attempt(&mut self, now: Instant) {
        self.last_enter = now;
    }

    fn drain_remaining(&mut self, receiver: &Receiver<Vec<u8>>) {
        while let Ok(data) = receiver.try_recv() {
            let remaining = MAX_CAPTURE_BYTES.saturating_sub(self.buffer.len());
            self.buffer
                .extend_from_slice(&data[..data.len().min(remaining)]);
        }
    }

    fn buffer(&self) -> &[u8] {
        &self.buffer
    }

    fn buffer_len(&self) -> usize {
        self.buffer.len()
    }

    fn into_buffer(self) -> Vec<u8> {
        self.buffer
    }
}

struct CaptureLoop<'a> {
    child: &'a mut ChildGuard,
    /// Held for the whole capture so auto-responses and periodic Enters can be
    /// written; `take_writer` is one-shot and cannot be called again.
    writer: &'a mut Box<dyn Write + Send>,
    options: &'a InteractiveOptions,
    receiver: Receiver<Vec<u8>>,
    state: CaptureState,
    prompt_responder: PromptResponder,
}

impl<'a> CaptureLoop<'a> {
    fn new(
        child: &'a mut ChildGuard,
        writer: &'a mut Box<dyn Write + Send>,
        options: &'a InteractiveOptions,
        receiver: Receiver<Vec<u8>>,
    ) -> Self {
        Self {
            child,
            writer,
            options,
            receiver,
            state: CaptureState::new(options.timeout, Instant::now()),
            prompt_responder: PromptResponder::new(&options.auto_responses),
        }
    }

    fn run(mut self) -> Result<Vec<u8>> {
        while self.state.should_continue(Instant::now()) {
            if !self.receive_reader_event() {
                break;
            }
            if self.process_exited()? {
                break;
            }
            if self.state.is_full() {
                log::warn!(
                    target: "interactive_runner",
                    "Output exceeded {} bytes; stopping capture and killing the child",
                    MAX_CAPTURE_BYTES
                );
                self.child.child_mut().kill()?;
                break;
            }
            if self.idle_timeout_reached() {
                break;
            }
            self.send_periodic_enter_if_due();
        }

        self.log_overall_timeout();
        self.state.drain_remaining(&self.receiver);
        Ok(self.state.into_buffer())
    }

    fn receive_reader_event(&mut self) -> bool {
        match self.receiver.recv_timeout(CAPTURE_POLL_INTERVAL) {
            Ok(data) => {
                self.handle_output_chunk(&data);
                true
            }
            Err(RecvTimeoutError::Timeout) => true,
            Err(RecvTimeoutError::Disconnected) => {
                log::info!(target: "interactive_runner", "Reader thread ended (EOF or error)");
                false
            }
        }
    }

    fn handle_output_chunk(&mut self, data: &[u8]) {
        self.state.record_chunk(data, Instant::now());
        let text = String::from_utf8_lossy(self.state.buffer()).into_owned();

        for prompt_match in self.prompt_responder.take_matches(&text) {
            self.send_auto_response(&prompt_match);
            self.state.record_prompt_response(Instant::now());
        }
    }

    fn send_auto_response(&mut self, prompt_match: &PromptMatch) {
        match self.writer.write_all(prompt_match.response.as_bytes()) {
            Ok(()) => {
                log::info!(
                    target: "interactive_runner",
                    "Auto-responded to normalized prompt '{}' with '{}'",
                    prompt_match.normalized_prompt,
                    prompt_match.response.trim()
                );
            }
            Err(e) => {
                log::warn!(
                    target: "interactive_runner",
                    "Auto-response matched normalized prompt '{}' but write failed: {}",
                    prompt_match.normalized_prompt, e
                );
            }
        }
    }

    fn process_exited(&mut self) -> Result<bool> {
        if let Some(_status) = self.child.child_mut().try_wait()? {
            log::info!(target: "interactive_runner", "Process exited");
            return Ok(true);
        }

        Ok(false)
    }

    fn idle_timeout_reached(&self) -> bool {
        if self
            .state
            .is_idle(self.options.idle_timeout, Instant::now())
        {
            log::info!(
                target: "interactive_runner",
                "Idle timeout reached after {:.1}s without new data, buffer: {} bytes",
                self.options.idle_timeout.as_secs_f64(),
                self.state.buffer_len()
            );
            return true;
        }

        false
    }

    fn send_periodic_enter_if_due(&mut self) {
        let Some(every) = self.options.send_enter_every else {
            return;
        };
        let now = Instant::now();
        if !self.state.should_send_enter(every, now) {
            return;
        }

        if self.writer.write_all(b"\r").is_err() {
            log::warn!(target: "interactive_runner", "Periodic Enter write failed");
        }
        self.state.record_enter_attempt(Instant::now());
    }

    fn log_overall_timeout(&self) {
        if self.state.timed_out(Instant::now()) {
            log::warn!(
                target: "interactive_runner",
                "Overall timeout ({:.0}s) reached, buffer: {} bytes",
                self.options.timeout.as_secs_f64(),
                self.state.buffer_len()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_delegates_to_text_utils() {
        // Verify the delegation works correctly
        assert_eq!(
            InteractiveRunner::normalize_for_matching("Hello World"),
            text_utils::normalize_for_matching("Hello World")
        );
    }

    /// The child must be reaped when the guard drops, even though nothing calls
    /// kill/wait explicitly. Regression test for the unreaped-child leak on the
    /// early-return paths (take_writer / write failure) in `run`.
    #[cfg(unix)]
    #[test]
    fn child_guard_reaps_on_drop() {
        let pty = native_pty_system();
        let pair = pty
            .openpty(PtySize {
                rows: 50,
                cols: 160,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("openpty");

        let mut cmd = CommandBuilder::new("/bin/sh");
        cmd.args(["-c", "sleep 60"]);
        let raw = pair.slave.spawn_command(cmd).expect("spawn");
        let pid = raw.process_id().expect("pid");
        let process_group_id = pair.master.process_group_leader();

        {
            let mut guard = ChildGuard::new(raw);
            guard.set_process_group(process_group_id);
            // dropped here without any explicit kill/wait
        }

        // The process must be gone (reaped), not left as a zombie. `kill -0`
        // succeeds for a zombie, so check the process state instead.
        let out = std::process::Command::new("ps")
            .args(["-o", "stat=", "-p", &pid.to_string()])
            .output()
            .expect("ps");
        let stat = String::from_utf8_lossy(&out.stdout).trim().to_string();
        assert!(
            stat.is_empty(),
            "child {} was left unreaped (ps stat = {:?})",
            pid,
            stat
        );
    }

    #[cfg(unix)]
    #[test]
    fn reader_thread_can_be_stopped_without_pty_eof() {
        let pty = native_pty_system();
        let pair = pty
            .openpty(PtySize {
                rows: 50,
                cols: 160,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("openpty");

        let mut reader = spawn_reader(&pair).expect("spawn reader");
        let _receiver = reader.take_receiver();
        reader.stop_and_join();
    }

    /// Stays alive after `kill`. A real child cannot: `Child::kill` ends in `SIGKILL`.
    #[derive(Debug)]
    struct NeverExitingChild;

    impl portable_pty::ChildKiller for NeverExitingChild {
        fn kill(&mut self) -> std::io::Result<()> {
            Ok(())
        }

        fn clone_killer(&self) -> Box<dyn portable_pty::ChildKiller + Send + Sync> {
            Box::new(Self)
        }
    }

    impl portable_pty::Child for NeverExitingChild {
        fn try_wait(&mut self) -> std::io::Result<Option<portable_pty::ExitStatus>> {
            Ok(None)
        }

        fn wait(&mut self) -> std::io::Result<portable_pty::ExitStatus> {
            Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "NeverExitingChild does not exit",
            ))
        }

        fn process_id(&self) -> Option<u32> {
            None
        }

        #[cfg(windows)]
        fn as_raw_handle(&self) -> Option<std::os::windows::io::RawHandle> {
            None
        }
    }

    /// Reap must give up after `REAP_TIMEOUT` when the child stays alive after kill.
    #[test]
    fn child_guard_reap_is_bounded() {
        const TEST_REAP_TIMEOUT: Duration = Duration::from_millis(50);
        let start = Instant::now();
        let mut guard = ChildGuard::new(Box::new(NeverExitingChild));
        guard.reap_with_timeout(TEST_REAP_TIMEOUT);
        let elapsed = start.elapsed();

        assert!(
            elapsed >= TEST_REAP_TIMEOUT,
            "reap returned in {:?}, before the {:?} give-up deadline",
            elapsed,
            TEST_REAP_TIMEOUT
        );
        assert!(
            elapsed < Duration::from_secs(1),
            "reap took {:?}, expected to stay below one second",
            elapsed,
        );
    }

    /// End-to-end: an auto-response must actually reach the child. Regression
    /// test for the one-shot `take_writer` bug, where every auto-response failed
    /// with "cannot take writer more than once".
    #[cfg(unix)]
    #[test]
    fn auto_response_reaches_child() {
        let dir = std::env::temp_dir().join(format!("runner_autoresp_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let script = dir.join("prompt.sh");
        std::fs::write(
            &script,
            "#!/bin/sh\nprintf 'Continue? [y/n] '\nread ans\nif [ \"$ans\" = \"y\" ]; then echo GOT_YES; else echo GOT_OTHER; fi\n",
        )
        .expect("write script");
        let mut perms = std::fs::metadata(&script).expect("stat").permissions();
        use std::os::unix::fs::PermissionsExt;
        perms.set_mode(0o755);
        std::fs::set_permissions(&script, perms).expect("chmod");

        let mut auto_responses = HashMap::new();
        auto_responses.insert("Continue? [y/n]".to_string(), "y\n".to_string());

        let runner = InteractiveRunner::new();
        let options = InteractiveOptions {
            timeout: Duration::from_secs(10),
            idle_timeout: Duration::from_secs(2),
            auto_responses,
            init_delay: Duration::from_millis(200),
            ..Default::default()
        };

        let result = runner
            .run(script.to_str().expect("path"), "", options)
            .expect("run should succeed");

        let _ = std::fs::remove_dir_all(&dir);

        assert!(
            result.output.contains("GOT_YES"),
            "auto-response did not reach the child; output was {:?}",
            result.output
        );
    }

    #[test]
    fn binary_not_found_error_mentions_binary() {
        let err = InteractiveError::BinaryNotFound("claude".to_string());
        assert!(err.to_string().contains("claude"));
    }

    #[test]
    fn prompt_responder_matches_normalized_full_output_once() {
        let mut auto_responses = HashMap::new();
        auto_responses.insert("Continue? [y/n]".to_string(), "y\n".to_string());
        let mut responder = PromptResponder::new(&auto_responses);

        let first_matches = responder.take_matches("\x1b[32mContinue?\x1b[0m   [y/n]");
        assert_eq!(first_matches.len(), 1);
        assert_eq!(first_matches[0].response, "y\n");

        let second_matches = responder.take_matches("Continue? [y/n]");
        assert!(second_matches.is_empty());
    }

    #[test]
    fn prompt_responder_matches_prompt_split_across_buffer_chunks() {
        let mut auto_responses = HashMap::new();
        auto_responses.insert(
            "Do you trust this directory?".to_string(),
            "y\n".to_string(),
        );
        let mut responder = PromptResponder::new(&auto_responses);

        assert!(responder.take_matches("Do you trust").is_empty());

        let matches = responder.take_matches("Do you trust this directory?");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].response, "y\n");
    }

    #[test]
    fn capture_state_tracks_idle_only_after_meaningful_data() {
        let start = Instant::now();
        let mut state = CaptureState::new(Duration::from_secs(10), start);

        state.record_chunk("\x1b7⠙\x1b8".as_bytes(), start + Duration::from_secs(1));
        assert!(state.is_idle(Duration::from_millis(500), start + Duration::from_secs(1)));

        state.record_chunk(b"ready", start + Duration::from_secs(2));
        assert!(!state.is_idle(Duration::from_secs(1), start + Duration::from_secs(2)));
        assert!(state.is_idle(Duration::from_secs(1), start + Duration::from_secs(4)));
    }

    #[test]
    fn capture_state_prompt_response_refreshes_idle_clock() {
        let start = Instant::now();
        let mut state = CaptureState::new(Duration::from_secs(10), start);

        state.record_chunk(b"prompt", start);
        state.record_prompt_response(start + Duration::from_secs(3));

        assert!(!state.is_idle(Duration::from_secs(2), start + Duration::from_secs(4)));
        assert!(state.is_idle(Duration::from_secs(2), start + Duration::from_secs(6)));
    }

    #[test]
    fn capture_state_tracks_periodic_enter_attempts() {
        let start = Instant::now();
        let mut state = CaptureState::new(Duration::from_secs(10), start);

        assert!(!state.should_send_enter(
            Duration::from_millis(500),
            start + Duration::from_millis(499)
        ));
        assert!(state.should_send_enter(
            Duration::from_millis(500),
            start + Duration::from_millis(500)
        ));

        state.record_enter_attempt(start + Duration::from_millis(500));
        assert!(!state.should_send_enter(
            Duration::from_millis(500),
            start + Duration::from_millis(999)
        ));
        assert!(state.should_send_enter(
            Duration::from_millis(500),
            start + Duration::from_millis(1000)
        ));
    }

    // ── From<InteractiveError> for ProviderError ──────────

    #[test]
    fn binary_not_found_maps_to_cli_not_found() {
        use crate::providers::ProviderError;
        let err: ProviderError = InteractiveError::BinaryNotFound("codex".into()).into();
        match err {
            ProviderError::CliNotFound { cli_name } => {
                assert_eq!(cli_name, "codex");
            }
            other => panic!("expected CliNotFound, got {:?}", other),
        }
    }

    #[test]
    fn pty_failed_maps_to_fetch_failed() {
        use crate::providers::ProviderError;
        let err: ProviderError = InteractiveError::PtyFailed("pipe error".into()).into();
        match err {
            ProviderError::FetchFailed { raw_detail, .. } => {
                let msg = raw_detail.expect("should have detail");
                assert!(
                    msg.contains("terminal session"),
                    "should contain readable message: {}",
                    msg
                );
            }
            other => panic!("expected FetchFailed, got {:?}", other),
        }
    }

    /// 缓冲达到上限后，后续块被静默丢弃，`is_full` 让捕获循环停止。
    #[test]
    fn capture_state_caps_buffer_at_max() {
        let now = Instant::now();
        let mut state = CaptureState::new(Duration::from_secs(60), now);

        // 分块喂入超过上限的数据。
        let chunk = vec![b'x'; MAX_CAPTURE_BYTES / 4];
        for _ in 0..5 {
            state.record_chunk(&chunk, now);
        }

        assert!(state.is_full(), "buffer must reach the cap");
        assert_eq!(state.buffer_len(), MAX_CAPTURE_BYTES);

        // 已满后再喂入不再增长。
        state.record_chunk(b"more", now);
        assert_eq!(state.buffer_len(), MAX_CAPTURE_BYTES);
    }

    /// `drain_remaining` 同样受上限约束（进程退出后的尾部冲刷不能超限）。
    #[test]
    fn capture_state_drain_respects_max() {
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(vec![b'y'; MAX_CAPTURE_BYTES]).unwrap();
        tx.send(vec![b'z'; 1024]).unwrap();

        let now = Instant::now();
        let mut state = CaptureState::new(Duration::from_secs(60), now);
        state.drain_remaining(&rx);

        assert!(state.is_full());
        assert_eq!(state.buffer_len(), MAX_CAPTURE_BYTES);
    }
}
