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
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::providers::common::path_resolver;
use crate::utils::text_utils;

const READER_CHUNK_SIZE: usize = 8192;
const CAPTURE_POLL_INTERVAL: Duration = Duration::from_millis(60);

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

        // Spawn process
        let mut child = self.spawn_process(&pair, &executable_path, &options)?;
        log::info!(
            target: "interactive_runner",
            "[{}] Process spawned ({:.0}ms), waiting {:.0}ms init delay",
            binary, start.elapsed().as_millis(), options.init_delay.as_millis()
        );

        // Allow process to initialize
        std::thread::sleep(options.init_delay);

        // Send input command
        if !input.trim().is_empty() {
            let mut writer = pair.master.take_writer()?;
            let input_data = format!("{}\r", input.trim());
            log::info!(
                target: "interactive_runner",
                "[{}] Sending input ({} bytes): {:?} ({:.0}ms)",
                binary, input_data.len(), input_data, start.elapsed().as_millis()
            );
            writer.write_all(input_data.as_bytes())?;
        }

        // Capture output with auto-response handling
        let buffer = self.capture_output(&pair, &mut child, &options)?;

        let elapsed = start.elapsed();
        log::debug!(
            target: "interactive_runner",
            "Command '{}' completed in {:.3}s, output length: {} bytes",
            binary,
            elapsed.as_secs_f64(),
            buffer.len()
        );

        if child.try_wait()?.is_none() {
            let _ = child.kill();
            let _ = child.wait();
        }

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
        child: &mut Box<dyn portable_pty::Child + Send + Sync>,
        options: &InteractiveOptions,
    ) -> Result<Vec<u8>> {
        let ReaderThread { receiver, handle } = spawn_reader(pair)?;
        let buffer = CaptureLoop::new(pair, child, options, receiver).run()?;
        drop(handle);
        Ok(buffer)
    }

    fn normalize_for_matching(text: &str) -> String {
        text_utils::normalize_for_matching(text)
    }
}

struct ReaderThread {
    receiver: Receiver<Vec<u8>>,
    handle: JoinHandle<()>,
}

fn spawn_reader(pair: &PtyPair) -> Result<ReaderThread> {
    let mut reader = pair.master.try_clone_reader()?;
    let (tx, rx) = mpsc::channel::<Vec<u8>>();

    let handle = std::thread::spawn(move || {
        let mut chunk = [0u8; READER_CHUNK_SIZE];
        loop {
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
        receiver: rx,
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

    fn record_chunk(&mut self, data: &[u8], now: Instant) {
        if text_utils::has_meaningful_content(data) {
            self.last_meaningful_data = now;
        }
        self.buffer.extend_from_slice(data);
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
            self.buffer.extend_from_slice(&data);
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
    pair: &'a PtyPair,
    child: &'a mut Box<dyn portable_pty::Child + Send + Sync>,
    options: &'a InteractiveOptions,
    receiver: Receiver<Vec<u8>>,
    state: CaptureState,
    prompt_responder: PromptResponder,
}

impl<'a> CaptureLoop<'a> {
    fn new(
        pair: &'a PtyPair,
        child: &'a mut Box<dyn portable_pty::Child + Send + Sync>,
        options: &'a InteractiveOptions,
        receiver: Receiver<Vec<u8>>,
    ) -> Self {
        Self {
            pair,
            child,
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

    fn send_auto_response(&self, prompt_match: &PromptMatch) {
        match self.pair.master.take_writer() {
            Ok(mut writer) => {
                let _ = writer.write_all(prompt_match.response.as_bytes());
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
                    "Auto-response matched normalized prompt '{}' but take_writer failed: {}",
                    prompt_match.normalized_prompt, e
                );
            }
        }
    }

    fn process_exited(&mut self) -> Result<bool> {
        if let Some(_status) = self.child.try_wait()? {
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

        if let Ok(mut writer) = self.pair.master.take_writer() {
            let _ = writer.write_all(b"\r");
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
}
