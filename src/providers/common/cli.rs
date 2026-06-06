use crate::models::FailureAdvice;
use crate::providers::common::path_resolver;
use crate::providers::ProviderError;
use anyhow::Result;
use std::io::Read;
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(20);
const COMMAND_POLL_INTERVAL: Duration = Duration::from_millis(50);

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
    let stdout_handle = child.stdout.take().map(|mut handle| {
        thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = handle.read_to_end(&mut buf);
            buf
        })
    });
    let stderr_handle = child.stderr.take().map(|mut handle| {
        thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = handle.read_to_end(&mut buf);
            buf
        })
    });

    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
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

fn join_reader_before_deadline(
    handle: Option<thread::JoinHandle<Vec<u8>>>,
    deadline: Instant,
    child: &mut Child,
) -> Result<Vec<u8>> {
    let Some(handle) = handle else {
        return Ok(Vec::new());
    };
    loop {
        if handle.is_finished() {
            return Ok(handle.join().unwrap_or_default());
        }
        if Instant::now() >= deadline {
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

/// 适用于"成功执行且输出在 stdout"的常规 CLI。
#[allow(dead_code)]
pub fn run_checked_command(binary: &str, args: &[&str]) -> Result<Output> {
    let output = run_command(binary, args)?;
    ensure_success(&output)?;
    Ok(output)
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
}
