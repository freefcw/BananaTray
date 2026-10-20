use crate::models::FailureAdvice;
use crate::providers::common::path_resolver;
use crate::providers::ProviderError;
use anyhow::Result;
use log::warn;
use std::io::Read;
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(20);
const COMMAND_POLL_INTERVAL: Duration = Duration::from_millis(50);
/// 单个流（stdout / stderr）的捕获上限。
///
/// 超过上限就杀掉子进程并报失败，而不是把整个输出读进内存。
const MAX_STREAM_BYTES: u64 = super::MAX_EXTERNAL_PAYLOAD_BYTES;

/// 检查 CLI 是否可执行。
pub fn command_exists(binary: &str) -> bool {
    path_resolver::locate_executable(binary).is_some()
}

/// 执行命令，并将"命令不存在"统一映射为 `CliNotFound`。
pub fn run_command(binary: &str, args: &[&str]) -> Result<Output> {
    run_command_with_timeout(binary, args, COMMAND_TIMEOUT)
}

pub fn run_command_with_timeout(binary: &str, args: &[&str], timeout: Duration) -> Result<Output> {
    let executable_path = path_resolver::locate_executable(binary)
        .ok_or_else(|| ProviderError::cli_not_found(binary))?;

    let mut command = Command::new(&executable_path);
    command
        .args(args)
        .env("PATH", path_resolver::enriched_path());
    prepare_process_group(&mut command);
    let child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| ProviderError::cli_not_found(binary))?;
    run_child_with_timeout(child, timeout)
}

#[allow(dead_code)] // 仅 app feature 下的 runtime/effects/script_provider.rs 调用
pub(crate) fn run_prepared_command_with_timeout(
    mut command: Command,
    timeout: Duration,
) -> Result<Output> {
    prepare_process_group(&mut command);
    let child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    run_child_with_timeout(child, timeout)
}

fn run_child_with_timeout(mut child: Child, timeout: Duration) -> Result<Output> {
    let stdout_handle = child
        .stdout
        .take()
        .map(|handle| thread::spawn(move || read_bounded(handle, MAX_STREAM_BYTES)));
    let stderr_handle = child
        .stderr
        .take()
        .map(|handle| thread::spawn(move || read_bounded(handle, MAX_STREAM_BYTES)));

    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            warn!(
                target: "cli",
                "command exceeded {:?}; killing the process tree",
                timeout
            );
            kill_child_tree(&mut child);
            let _ = child.wait();
            return Err(ProviderError::Timeout.into());
        }
        thread::sleep(COMMAND_POLL_INTERVAL);
    };

    let stdout = join_reader_before_deadline(stdout_handle, deadline, &mut child)?;
    let stderr = join_reader_before_deadline(stderr_handle, deadline, &mut child)?;

    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

/// 读取一个流，最多 `limit` 字节；超出则返回 `Err(())` 让调用方杀掉子进程。
fn read_bounded(mut handle: impl Read, limit: u64) -> std::result::Result<Vec<u8>, ()> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    let mut total: u64 = 0;
    loop {
        match handle.read(&mut chunk) {
            Ok(0) => return Ok(buf),
            Ok(n) => {
                total += n as u64;
                if total > limit {
                    return Err(());
                }
                buf.extend_from_slice(&chunk[..n]);
            }
            Err(_) => return Ok(buf),
        }
    }
}

fn join_reader_before_deadline(
    handle: Option<thread::JoinHandle<std::result::Result<Vec<u8>, ()>>>,
    deadline: Instant,
    child: &mut Child,
) -> Result<Vec<u8>> {
    let Some(handle) = handle else {
        return Ok(Vec::new());
    };
    loop {
        if handle.is_finished() {
            return match handle.join().unwrap_or(Err(())) {
                Ok(buf) => Ok(buf),
                Err(()) => {
                    // 输出超过上限：杀掉子进程并报失败，不返回半截输出。
                    warn!(
                        target: "cli",
                        "command output exceeded the {} byte limit; killing the process",
                        MAX_STREAM_BYTES
                    );
                    kill_child_tree(child);
                    let _ = child.wait();
                    Err(
                        ProviderError::fetch_failed("command output exceeded the size limit")
                            .into(),
                    )
                }
            };
        }
        if Instant::now() >= deadline {
            warn!(
                target: "cli",
                "command did not finish draining output before the deadline; killing the process tree"
            );
            kill_child_tree(child);
            let _ = child.wait();
            return Err(ProviderError::Timeout.into());
        }
        thread::sleep(COMMAND_POLL_INTERVAL);
    }
}

#[cfg(unix)]
fn prepare_process_group(command: &mut Command) {
    use std::os::unix::process::CommandExt;

    unsafe {
        command.pre_exec(|| {
            if libc::setpgid(0, 0) == -1 {
                Err(std::io::Error::last_os_error())
            } else {
                Ok(())
            }
        });
    }
}

#[cfg(not(unix))]
fn prepare_process_group(_command: &mut Command) {}

#[cfg(unix)]
fn kill_child_tree(child: &mut Child) {
    if let Ok(pgid) = libc::pid_t::try_from(child.id()) {
        unsafe {
            let _ = libc::kill(-pgid, libc::SIGKILL);
        }
    }
    let _ = child.kill();
}

#[cfg(not(unix))]
fn kill_child_tree(child: &mut Child) {
    let _ = child.kill();
}

/// 统一处理非零退出码，避免各个 CLI provider 重复拼接错误文案。
pub fn ensure_success(output: &Output) -> Result<()> {
    if output.status.success() {
        return Ok(());
    }

    Err(
        ProviderError::fetch_failed_with_advice(FailureAdvice::CliExitFailed {
            code: output.status.code().unwrap_or(-1),
        })
        .into(),
    )
}

/// 适用于偶发非零退出码但仍有有效输出的 CLI（如 amp、kiro-cli）。
/// 有输出时直接返回，仅在输出为空时才将非零退出码视为错误。
pub fn run_lenient_command(binary: &str, args: &[&str]) -> Result<String> {
    run_lenient_command_with_timeout(binary, args, None)
}

pub fn run_lenient_command_with_timeout(
    binary: &str,
    args: &[&str],
    timeout: Option<Duration>,
) -> Result<String> {
    let output = match timeout {
        Some(timeout) => run_command_with_timeout(binary, args, timeout)?,
        None => run_command(binary, args)?,
    };
    let text = stdout_or_stderr_text(&output);
    if text.trim().is_empty() {
        ensure_success(&output)?;
    }
    Ok(text)
}

pub fn stdout_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// 某些 CLI 会把业务输出写到 stderr，这里提供统一兜底。
pub fn stdout_or_stderr_text(output: &Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.trim().is_empty() {
        String::from_utf8_lossy(&output.stderr).into_owned()
    } else {
        stdout.into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn make_status(code: i32) -> std::process::ExitStatus {
        std::os::unix::process::ExitStatusExt::from_raw(code << 8)
    }

    #[cfg(windows)]
    fn make_status(code: i32) -> std::process::ExitStatus {
        std::os::windows::process::ExitStatusExt::from_raw(code as u32)
    }

    fn success_status() -> std::process::ExitStatus {
        make_status(0)
    }

    fn failure_status() -> std::process::ExitStatus {
        make_status(1)
    }

    #[test]
    fn test_stdout_or_stderr_prefers_stdout() {
        let output = Output {
            status: success_status(),
            stdout: b"main output".to_vec(),
            stderr: b"fallback output".to_vec(),
        };
        assert_eq!(stdout_or_stderr_text(&output), "main output");
    }

    #[test]
    fn test_stdout_or_stderr_uses_stderr_when_stdout_empty() {
        let output = Output {
            status: success_status(),
            stdout: b"   ".to_vec(),
            stderr: b"fallback output".to_vec(),
        };
        assert_eq!(stdout_or_stderr_text(&output), "fallback output");
    }

    #[test]
    fn test_run_lenient_returns_output_even_on_nonzero_exit() {
        // 有输出时，即使退出码非零也应返回 Ok
        let output = Output {
            status: failure_status(),
            stdout: b"quota: 100/200".to_vec(),
            stderr: b"some warning".to_vec(),
        };
        let text = stdout_or_stderr_text(&output);
        // 模拟 run_lenient_command 的核心逻辑
        let result: Result<String> = if text.trim().is_empty() {
            ensure_success(&output).map(|_| text)
        } else {
            Ok(text)
        };
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "quota: 100/200");
    }

    #[test]
    fn test_run_lenient_fails_when_output_empty_and_nonzero_exit() {
        // 无输出且退出码非零时应返回 Err
        let output = Output {
            status: failure_status(),
            stdout: b"".to_vec(),
            stderr: b"".to_vec(),
        };
        let text = stdout_or_stderr_text(&output);
        let result: Result<String> = if text.trim().is_empty() {
            ensure_success(&output).map(|_| text)
        } else {
            Ok(text)
        };
        assert!(result.is_err());
    }

    #[test]
    fn test_enriched_path_contains_home_paths() {
        let path = path_resolver::enriched_path();
        assert!(!path.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn test_run_command_with_timeout_returns_timeout_error() {
        let err = run_command_with_timeout("sh", &["-c", "sleep 1"], Duration::from_millis(50))
            .unwrap_err();
        let classified = ProviderError::classify(&err);
        assert!(matches!(classified, ProviderError::Timeout));
    }

    #[cfg(unix)]
    #[test]
    fn test_run_command_with_timeout_handles_large_stdout() {
        let output = run_command_with_timeout(
            "sh",
            &["-c", "yes x | head -n 100000"],
            Duration::from_secs(2),
        )
        .unwrap();
        let stdout = stdout_text(&output);
        assert!(stdout.lines().count() >= 100000);
    }

    #[cfg(unix)]
    #[test]
    fn test_run_command_with_timeout_covers_reader_drain() {
        let err = run_command_with_timeout(
            "sh",
            &["-c", "sh -c 'sleep 1 & exit 0'"],
            Duration::from_millis(50),
        )
        .unwrap_err();
        let classified = ProviderError::classify(&err);
        assert!(matches!(classified, ProviderError::Timeout));
    }

    /// 狂吐输出的命令必须在超过上限时失败，而不是把整个输出读进内存。
    #[cfg(unix)]
    #[test]
    fn test_run_command_with_timeout_rejects_output_over_limit() {
        // 持续输出直到被杀，远超 MAX_STREAM_BYTES。
        let err = run_command_with_timeout(
            "sh",
            &["-c", "yes 0123456789abcdef"],
            Duration::from_secs(10),
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("size limit"),
            "unexpected error: {err}"
        );
    }
}
