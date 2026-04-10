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
use std::time::{Duration, Instant};

use crate::utils::text_utils;

/// Result of running an interactive command
#[derive(Debug)]
#[allow(dead_code)]
pub struct InteractiveResult {
    /// The captured output from the command
    pub output: String,
    /// The command's exit code (None if still running or couldn't determine)
    pub exit_code: Option<i32>,
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
#[allow(dead_code)]
pub enum InteractiveError {
    /// CLI tool not found
    BinaryNotFound(String),
    /// Failed to create PTY
    PtyFailed(String),
    /// Failed to start command
    LaunchFailed(String),
    /// Command timed out
    TimedOut,
    /// Process exited unexpectedly
    ProcessExited,
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
            Self::TimedOut => write!(f, "Command did not complete within the timeout."),
            Self::ProcessExited => write!(f, "Process exited unexpectedly."),
        }
    }
}

impl std::error::Error for InteractiveError {}

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
    /// The captured output and exit code
    pub fn run(
        &self,
        binary: &str,
        input: &str,
        options: InteractiveOptions,
    ) -> Result<InteractiveResult> {
        let start = Instant::now();

        // Find executable
        let executable_path = self.find_executable(binary)?;
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

        // Get exit status
        let exit_code = match child.try_wait()? {
            Some(status) => Some(status.exit_code() as i32),
            None => {
                // Process still running, kill it
                let _ = child.kill();
                child.wait().map(|s| s.exit_code() as i32).ok()
            }
        };

        // Strip ANSI codes from output
        let output = text_utils::strip_ansi(&String::from_utf8_lossy(&buffer));

        Ok(InteractiveResult { output, exit_code })
    }

    /// Find the full path to a CLI tool
    fn find_executable(&self, binary: &str) -> Result<String> {
        // Check if it's already an absolute path and exists
        if std::path::Path::new(binary).is_absolute()
            && std::fs::metadata(binary)
                .map(|m| m.is_file())
                .unwrap_or(false)
        {
            return Ok(binary.to_string());
        }

        // Use 'which' to find the binary
        if let Ok(path) = which::which(binary) {
            return Ok(path.to_string_lossy().to_string());
        }

        // Fallback: check common paths
        let common_paths = ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"];

        for base in &common_paths {
            let full_path = format!("{}/{}", base, binary);
            if std::fs::metadata(&full_path)
                .map(|m| m.is_file())
                .unwrap_or(false)
            {
                return Ok(full_path);
            }
        }

        Err(InteractiveError::BinaryNotFound(binary.to_string()).into())
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

        // Ensure common paths are included in PATH
        if let Some(path) = env.get_mut("PATH") {
            *path = Self::ensure_common_paths(path);
        }

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

    /// Ensure common tool paths are included in PATH
    fn ensure_common_paths(path: &str) -> String {
        let essential_paths = [
            "/opt/homebrew/bin",
            "/opt/homebrew/sbin",
            "/usr/local/bin",
            "/usr/local/sbin",
        ];

        let mut components: Vec<&str> = path.split(':').collect();

        for essential_path in essential_paths.iter().rev() {
            if !components.contains(essential_path) {
                let p = std::path::Path::new(essential_path);
                if p.exists() {
                    components.insert(0, essential_path);
                }
            }
        }

        components.join(":")
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
        let mut reader = pair.master.try_clone_reader()?;

        // Spawn a reader thread that sends chunks via channel, avoiding blocking read
        // in the main loop which would prevent timeout/idle checks from executing.
        let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
        let reader_handle = std::thread::spawn(move || {
            let mut chunk = [0u8; 8192];
            loop {
                match reader.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => {
                        if tx.send(chunk[..n].to_vec()).is_err() {
                            break; // receiver dropped
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        let deadline = Instant::now() + options.timeout;
        let mut buffer = Vec::new();
        let mut last_meaningful_data = Instant::now();
        let mut responded_prompts = HashSet::new();
        let mut last_enter = Instant::now();

        // Prepare prompt-response pairs (normalized for matching)
        let prompt_responses: Vec<(String, String)> = options
            .auto_responses
            .iter()
            .map(|(k, v)| (Self::normalize_for_matching(k), v.clone()))
            .collect();

        while Instant::now() < deadline {
            // Non-blocking receive from reader thread
            match rx.recv_timeout(Duration::from_millis(60)) {
                Ok(data) => {
                    // Check if this is meaningful data
                    if self.is_meaningful_data(&data) {
                        last_meaningful_data = Instant::now();
                    }
                    buffer.extend_from_slice(&data);

                    // Check for auto-response triggers
                    let text = String::from_utf8_lossy(&buffer);
                    let normalized = Self::normalize_for_matching(&text);

                    for (prompt, response) in &prompt_responses {
                        if !responded_prompts.contains(prompt) && normalized.contains(prompt) {
                            match pair.master.take_writer() {
                                Ok(mut writer) => {
                                    let _ = writer.write_all(response.as_bytes());
                                    log::info!(
                                        target: "interactive_runner",
                                        "Auto-responded to prompt '{}' with '{}'",
                                        prompt,
                                        response.trim()
                                    );
                                }
                                Err(e) => {
                                    log::warn!(
                                        target: "interactive_runner",
                                        "Auto-response matched '{}' but take_writer failed: {}",
                                        prompt, e
                                    );
                                }
                            }
                            responded_prompts.insert(prompt.clone());
                            last_meaningful_data = Instant::now();
                        }
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    // No data received in this interval, fall through to checks
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    log::info!(target: "interactive_runner", "Reader thread ended (EOF or error)");
                    break;
                }
            }

            // Check if process has exited
            if let Some(_status) = child.try_wait()? {
                log::info!(target: "interactive_runner", "Process exited");
                break;
            }

            // Check idle timeout
            if !buffer.is_empty()
                && Instant::now().duration_since(last_meaningful_data) > options.idle_timeout
            {
                log::info!(
                    target: "interactive_runner",
                    "Idle timeout reached after {:.1}s without new data, buffer: {} bytes",
                    options.idle_timeout.as_secs_f64(),
                    buffer.len()
                );
                break;
            }

            // Send periodic Enter if configured
            if let Some(every) = options.send_enter_every {
                if Instant::now().duration_since(last_enter) >= every {
                    if let Ok(mut writer) = pair.master.take_writer() {
                        let _ = writer.write_all(b"\r");
                    }
                    last_enter = Instant::now();
                }
            }
        }

        if Instant::now() >= deadline {
            log::warn!(
                target: "interactive_runner",
                "Overall timeout ({:.0}s) reached, buffer: {} bytes",
                options.timeout.as_secs_f64(),
                buffer.len()
            );
        }

        // Drain any remaining data from channel (non-blocking)
        while let Ok(data) = rx.try_recv() {
            buffer.extend_from_slice(&data);
        }

        // Don't wait for reader thread — it will be cleaned up when the PTY master is dropped
        drop(reader_handle);

        Ok(buffer)
    }

    fn normalize_for_matching(text: &str) -> String {
        text_utils::normalize_for_matching(text)
    }

    fn is_meaningful_data(&self, data: &[u8]) -> bool {
        text_utils::has_meaningful_content(data)
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
    fn test_ensure_common_paths() {
        let path = "/usr/bin";
        let result = InteractiveRunner::ensure_common_paths(path);

        // Should include /usr/bin
        assert!(result.contains("/usr/bin"));
    }

    #[test]
    fn test_interactive_error_display() {
        let err = InteractiveError::BinaryNotFound("claude".to_string());
        assert!(err.to_string().contains("claude"));

        let err = InteractiveError::TimedOut;
        assert!(err.to_string().contains("timeout"));
    }
}
