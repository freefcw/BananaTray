use crate::models::FailureAdvice;
use crate::providers::common::path_resolver;
use crate::providers::ProviderError;
use anyhow::Result;
use std::io::Read;
use std::process::{Command, Output, Stdio};
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

fn run_command_with_timeout(binary: &str, args: &[&str], timeout: Duration) -> Result<Output> {
    let executable_path = path_resolver::locate_executable(binary)
        .ok_or_else(|| ProviderError::cli_not_found(binary))?;

    let mut child = Command::new(&executable_path)
        .args(args)
        .env("PATH", path_resolver::enriched_path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| ProviderError::cli_not_found(binary))?;

    let stdout_reader = child.stdout.take().map(|mut handle| {
        thread::spawn(move || {
            let mut stdout = Vec::new();
            let _ = handle.read_to_end(&mut stdout);
            stdout
        })
    });
    let stderr_reader = child.stderr.take().map(|mut handle| {
        thread::spawn(move || {
            let mut stderr = Vec::new();
            let _ = handle.read_to_end(&mut stderr);
            stderr
        })
    });

    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(ProviderError::Timeout.into());
        }
        thread::sleep(COMMAND_POLL_INTERVAL);
    };

    let stdout = stdout_reader
        .map(|reader| reader.join().unwrap_or_default())
        .unwrap_or_default();
    let stderr = stderr_reader
        .map(|reader| reader.join().unwrap_or_default())
        .unwrap_or_default();

    Ok(Output {
        status,
        stdout,
        stderr,
    })
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
    let output = run_command(binary, args)?;
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
}
