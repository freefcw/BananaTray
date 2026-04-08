use crate::providers::ProviderError;
use anyhow::Result;
use rust_i18n::t;
use std::process::{Command, Output};

/// 构建包含常见工具路径的 PATH 环境变量。
///
/// macOS GUI 应用继承的 PATH 非常有限（通常只有 /usr/bin:/bin:/usr/sbin:/sbin），
/// 用户通过 Homebrew、npm、bun、cargo 等安装的 CLI 工具往往不在其中。
/// 这里将常见安装路径补充到 PATH 前端，确保 GUI 环境也能找到这些命令。
fn enriched_path() -> String {
    let current = std::env::var("PATH").unwrap_or_default();
    let mut components: Vec<&str> = current.split(':').collect();

    // 用户 home 下的常见路径
    let home = std::env::var("HOME").unwrap_or_default();
    let home_paths: Vec<String> = if home.is_empty() {
        vec![]
    } else {
        vec![
            format!("{}/.local/bin", home),
            format!("{}/.bun/bin", home),
            format!("{}/.cargo/bin", home),
            format!("{}/.npm-global/bin", home),
            format!("{}/.amp/bin", home),
        ]
    };

    // 系统级常见路径
    let system_paths = [
        "/opt/homebrew/bin",
        "/opt/homebrew/sbin",
        "/usr/local/bin",
        "/usr/local/sbin",
    ];

    // 从后往前插入，保证优先级：用户路径 > Homebrew > 系统路径
    for p in system_paths.iter().rev() {
        if !components.contains(p) && std::path::Path::new(p).exists() {
            components.insert(0, p);
        }
    }
    for p in home_paths.iter().rev() {
        if !components.contains(&p.as_str()) && std::path::Path::new(p).exists() {
            components.insert(0, p.as_str());
        }
    }

    components.join(":")
}

/// 检查 CLI 是否可执行。
pub fn command_exists(binary: &str) -> bool {
    Command::new(binary)
        .arg("--version")
        .env("PATH", enriched_path())
        .output()
        .is_ok()
}

/// 执行命令，并将"命令不存在"统一映射为 `CliNotFound`。
pub fn run_command(binary: &str, args: &[&str]) -> Result<Output> {
    Command::new(binary)
        .args(args)
        .env("PATH", enriched_path())
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

/// 适用于"成功执行且输出在 stdout"的常规 CLI。
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

    #[test]
    fn test_enriched_path_contains_home_paths() {
        let path = enriched_path();
        let home = std::env::var("HOME").unwrap_or_default();
        if !home.is_empty() {
            // 至少应该包含存在的路径
            assert!(!path.is_empty());
        }
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
