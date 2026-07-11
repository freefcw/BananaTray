use anyhow::{anyhow, bail, Context, Result};
use std::io::Write;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::OnceLock;

/// 使用系统默认浏览器打开外部 URL
///
/// 跨平台支持：macOS → `open`，Linux → `xdg-open`
pub fn open_url(url: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let mut command = Command::new("open");
        command.arg(url);
        spawn_checked_command(&mut command).map(|_| ())
    }
    #[cfg(target_os = "linux")]
    {
        let mut command = Command::new("xdg-open");
        command.arg(url);
        spawn_checked_command(&mut command).map(|_| ())
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        bail!("opening URLs is not supported on this platform")
    }
}

/// 在系统文件管理器中打开指定路径（显示该文件所在目录）
///
/// macOS: `open -R <path>`（在 Finder 中选中文件）
/// Linux: `xdg-open <parent_dir>`
pub fn open_path_in_finder(path: &Path) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let mut command = Command::new("open");
        command.arg("-R").arg(path);
        spawn_checked_command(&mut command).map(|_| ())
    }
    #[cfg(target_os = "linux")]
    {
        let dir = path.parent().unwrap_or(path);
        let mut command = Command::new("xdg-open");
        command.arg(dir);
        spawn_checked_command(&mut command).map(|_| ())
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        bail!("opening paths is not supported on this platform")
    }
}

/// 将文本写入系统剪贴板
///
/// macOS: `pbcopy`
/// Linux: `xclip` 或 `xsel`
pub fn copy_to_clipboard(text: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        copy_with_candidates(
            text,
            &[CommandSpec {
                program: "pbcopy",
                args: &[],
            }],
            run_command_with_input,
        )
    }
    #[cfg(target_os = "linux")]
    {
        copy_with_candidates(
            text,
            &[
                CommandSpec {
                    program: "xclip",
                    args: &["-selection", "clipboard"],
                },
                CommandSpec {
                    program: "xsel",
                    args: &["--clipboard"],
                },
            ],
            run_command_with_input,
        )
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        bail!("clipboard integration is not supported on this platform")
    }
}

#[derive(Clone, Copy)]
struct CommandSpec<'a> {
    program: &'a str,
    args: &'a [&'a str],
}

fn copy_with_candidates(
    text: &str,
    candidates: &[CommandSpec<'_>],
    mut run: impl FnMut(&str, &[&str], &str) -> Result<()>,
) -> Result<()> {
    let mut failures = Vec::new();
    for candidate in candidates {
        match run(candidate.program, candidate.args, text) {
            Ok(()) => return Ok(()),
            Err(err) => failures.push(format!("{}: {err:#}", candidate.program)),
        }
    }

    Err(anyhow!(
        "all clipboard commands failed: {}",
        failures.join("; ")
    ))
}

fn run_command_with_input(program: &str, args: &[&str], text: &str) -> Result<()> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to start {program}"))?;

    let write_result = child
        .stdin
        .take()
        .context("child process did not expose stdin")?
        .write_all(text.as_bytes())
        .with_context(|| format!("failed to write to {program}"));
    let status = child
        .wait()
        .with_context(|| format!("failed to wait for {program}"))?;

    write_result?;
    ensure_success(program, status)
}

fn spawn_checked_command(command: &mut Command) -> Result<std::thread::JoinHandle<()>> {
    let description = format!("{command:?}");
    let child = command
        .spawn()
        .with_context(|| format!("failed to start {description}"))?;
    let monitor_description = description.clone();
    let mut child = MonitoredChild::new(child);

    std::thread::Builder::new()
        .name("system-command-monitor".into())
        .spawn(move || {
            let result = child
                .wait()
                .with_context(|| format!("failed to wait for {description}"))
                .and_then(|status| ensure_success(&description, status));
            if let Err(err) = result {
                log::warn!(target: "app", "external command failed: {err:#}");
            }
        })
        .with_context(|| format!("failed to start monitor for {monitor_description}"))
}

struct MonitoredChild {
    child: Option<Child>,
}

impl MonitoredChild {
    fn new(child: Child) -> Self {
        Self { child: Some(child) }
    }

    fn wait(&mut self) -> std::io::Result<ExitStatus> {
        let status = self
            .child
            .as_mut()
            .expect("monitored child must exist before wait")
            .wait()?;
        self.child = None;
        Ok(status)
    }
}

impl Drop for MonitoredChild {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn ensure_success(command: &str, status: ExitStatus) -> Result<()> {
    if status.success() {
        Ok(())
    } else {
        bail!("{command} exited with status {status}")
    }
}

/// 获取操作系统版本信息字符串。
///
/// 系统版本在进程生命周期内不会变化，因此首次探测后复用缓存。
/// macOS: `macOS 15.4 (aarch64)`
/// Linux: `Linux (x86_64)`
pub fn os_info() -> String {
    static OS_INFO: OnceLock<String> = OnceLock::new();
    OS_INFO.get_or_init(detect_os_info).clone()
}

fn detect_os_info() -> String {
    let arch = std::env::consts::ARCH;

    #[cfg(target_os = "macos")]
    {
        let version = std::process::Command::new("sw_vers")
            .arg("-productVersion")
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|o| {
                let v = String::from_utf8_lossy(&o.stdout).trim().to_string();
                if v.is_empty() {
                    None
                } else {
                    Some(v)
                }
            })
            .unwrap_or_else(|| "unknown".to_string());
        format!("macOS {} ({})", version, arch)
    }
    #[cfg(target_os = "linux")]
    {
        format!("Linux ({})", arch)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        format!("{} ({})", std::env::consts::OS, arch)
    }
}

/// 将文件大小格式化为人类可读的字符串
pub fn format_file_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// 检测系统是否处于深色模式
///
/// macOS: 读取 `defaults read -g AppleInterfaceStyle`
/// Linux: 优先读取 GNOME `color-scheme`，fallback 到 GTK 主题名
pub fn detect_system_dark_mode() -> bool {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("defaults")
            .args(["read", "-g", "AppleInterfaceStyle"])
            .output()
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .trim()
                    .eq_ignore_ascii_case("dark")
            })
            .unwrap_or(false)
    }
    #[cfg(target_os = "linux")]
    {
        detect_linux_dark_mode()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        false
    }
}

/// Linux 深色模式检测
///
/// 1. `org.gnome.desktop.interface color-scheme` → GNOME 42+ 标准
/// 2. `org.gnome.desktop.interface gtk-theme` → 主题名含 "dark" 的 fallback
#[cfg(target_os = "linux")]
fn detect_linux_dark_mode() -> bool {
    // 方法 1: GNOME color-scheme（'prefer-dark' = 深色）
    if let Ok(output) = Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "color-scheme"])
        .output()
    {
        if output.status.success() {
            let value = String::from_utf8_lossy(&output.stdout);
            if value.contains("prefer-dark") {
                return true;
            }
            // 如果返回了有效值（如 'default'），说明 gsettings 可用但不是深色
            if value.contains("default") || value.contains("prefer-light") {
                return false;
            }
        }
    }

    // 方法 2: GTK 主题名是否包含 "dark"
    if let Ok(output) = Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "gtk-theme"])
        .output()
    {
        if output.status.success() {
            let theme = String::from_utf8_lossy(&output.stdout)
                .trim()
                .to_lowercase();
            if theme.contains("dark") {
                return true;
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_info_returns_non_empty() {
        let info = os_info();
        assert!(!info.is_empty());
        assert!(info.contains(std::env::consts::ARCH));
    }

    #[test]
    fn format_file_size_units() {
        assert_eq!(format_file_size(500), "500 B");
        assert_eq!(format_file_size(1024), "1.0 KB");
        assert_eq!(format_file_size(1536), "1.5 KB");
        assert_eq!(format_file_size(1048576), "1.0 MB");
        assert_eq!(format_file_size(2621440), "2.5 MB");
    }

    #[test]
    fn clipboard_candidates_fall_back_after_any_command_failure() {
        let candidates = [
            CommandSpec {
                program: "first",
                args: &[],
            },
            CommandSpec {
                program: "second",
                args: &[],
            },
        ];
        let mut attempted = Vec::new();

        let result = copy_with_candidates("payload", &candidates, |program, _, text| {
            attempted.push((program.to_string(), text.to_string()));
            if program == "first" {
                bail!("non-zero exit")
            }
            Ok(())
        });

        assert!(result.is_ok());
        assert_eq!(
            attempted,
            vec![
                ("first".to_string(), "payload".to_string()),
                ("second".to_string(), "payload".to_string())
            ]
        );
    }

    #[test]
    fn command_helpers_reject_non_zero_exit_status() {
        let status = Command::new("sh").args(["-c", "exit 7"]).status().unwrap();

        let err = ensure_success("sh", status).unwrap_err().to_string();

        assert!(err.contains("status"));
        assert!(err.contains('7'));
    }

    #[test]
    fn checked_command_spawn_returns_before_process_exit() {
        let dir = tempfile::tempdir().unwrap();
        let release_path = dir.path().join("release");
        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg(
                "i=0; while [ ! -e \"$1\" ] && [ \"$i\" -lt 500 ]; do \
                 sleep 0.01; i=$((i + 1)); done",
            )
            .arg("sh")
            .arg(&release_path);

        let (spawn_result_tx, spawn_result_rx) = std::sync::mpsc::channel();
        let caller = std::thread::spawn(move || {
            let result = spawn_checked_command(&mut command);
            let _ = spawn_result_tx.send(result);
        });

        let monitor = match spawn_result_rx.recv_timeout(std::time::Duration::from_secs(1)) {
            Ok(result) => result.unwrap(),
            Err(err) => {
                std::fs::write(&release_path, b"release").unwrap();
                panic!("spawn_checked_command blocked until process exit: {err}");
            }
        };

        caller.join().unwrap();
        assert!(!monitor.is_finished());
        std::fs::write(&release_path, b"release").unwrap();

        let (monitor_result_tx, monitor_result_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = monitor_result_tx.send(monitor.join());
        });
        monitor_result_rx
            .recv_timeout(std::time::Duration::from_secs(3))
            .expect("system command monitor did not finish after child release")
            .unwrap();
    }
}
