use crate::providers::ProviderError;
use anyhow::Result;
use rust_i18n::t;
use std::process::{Command, Output};

/// 检查 CLI 是否可执行。
pub fn command_exists(binary: &str) -> bool {
    Command::new(binary).arg("--version").output().is_ok()
}

/// 执行命令，并将“命令不存在”统一映射为 `CliNotFound`。
pub fn run_command(binary: &str, args: &[&str]) -> Result<Output> {
    Command::new(binary)
        .args(args)
        .output()
        .map_err(|_| ProviderError::cli_not_found(binary).into())
}

/// 统一处理非零退出码，避免各个 CLI provider 重复拼接错误文案。
pub fn ensure_success(output: &Output) -> Result<()> {
    if output.status.success() {
        return Ok(());
    }

    Err(ProviderError::fetch_failed(&t!(
        "hint.cli_exit_failed",
        code = output.status.code().unwrap_or(-1)
    ))
    .into())
}

/// 适用于“成功执行且输出在 stdout”的常规 CLI。
pub fn run_checked_command(binary: &str, args: &[&str]) -> Result<Output> {
    let output = run_command(binary, args)?;
    ensure_success(&output)?;
    Ok(output)
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

    #[cfg(unix)]
    fn success_status() -> std::process::ExitStatus {
        std::os::unix::process::ExitStatusExt::from_raw(0)
    }

    #[cfg(windows)]
    fn success_status() -> std::process::ExitStatus {
        std::os::windows::process::ExitStatusExt::from_raw(0)
    }
}
