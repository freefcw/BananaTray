use anyhow::{anyhow, bail, Context, Result};
use std::io::{self, Write};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::OnceLock;

#[derive(Default)]
pub(crate) struct NonBlockingBoolProbe {
    state: std::sync::Arc<std::sync::Mutex<BoolProbeState>>,
}

#[derive(Default)]
struct BoolProbeState {
    cached: Option<(std::time::Instant, bool)>,
    running: bool,
}

impl NonBlockingBoolProbe {
    #[allow(dead_code)] // app 启动前的 prewarm wiring 在 app feature 路径调用
    pub(crate) fn publish(&self, value: bool) {
        if let Ok(mut state) = self.state.lock() {
            state.cached = Some((std::time::Instant::now(), value));
            state.running = false;
        }
    }

    /// 返回最近缓存；过期或首次探测只启动一个后台线程，调用方从不等待命令/I/O。
    pub(crate) fn get_or_refresh(
        &self,
        fallback: bool,
        ttl: std::time::Duration,
        detect: impl FnOnce() -> bool + Send + 'static,
    ) -> bool {
        let now = std::time::Instant::now();
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(_) => return fallback,
        };
        if let Some((updated_at, value)) = state.cached {
            if now.duration_since(updated_at) < ttl {
                return value;
            }
        }
        let current = state.cached.map(|(_, value)| value).unwrap_or(fallback);
        if state.running {
            return current;
        }
        state.running = true;
        drop(state);

        let shared = self.state.clone();
        if std::thread::Builder::new()
            .name("system-bool-probe".into())
            .spawn(move || {
                let value = std::panic::catch_unwind(std::panic::AssertUnwindSafe(detect))
                    .unwrap_or(fallback);
                if let Ok(mut state) = shared.lock() {
                    state.cached = Some((std::time::Instant::now(), value));
                    state.running = false;
                }
            })
            .is_err()
        {
            if let Ok(mut state) = self.state.lock() {
                state.running = false;
            }
        }
        current
    }
}

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

/// 剪贴板输入和子进程退出各自等待的上限。
///
/// 正常情况下这个上限不会被触及：`xclip` / `xsel` / `wl-copy` 默认都会 daemon 化
/// （见 `xclip.c` 的 `xcverb == OSILENT` 分支、`xsel --nodetach` 的反向说明），
/// 父进程写完 stdin 后立即退出，`wait` 很快返回。
///
/// 保留超时是纯防御性的：若某个打包版本或未来的工具没有 daemon 化而是留在前台
/// 持有选区，调用线程会永久卡住，用户只能强杀进程。2 秒足够区分两种情况。
const CLIPBOARD_COMMAND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

/// 将文本写入系统剪贴板
///
/// macOS: `pbcopy`
/// Linux: 按会话类型选择 —— Wayland 优先 `wl-copy`，X11 优先 `xclip` / `xsel`。
/// 这个区分是必要的：`xclip` / `xsel` 走 X11 选区协议，在纯 Wayland 会话下
/// 完全无法工作，只列它们会让 GNOME/KDE 默认会话下的复制按钮直接报错。
///
/// 实现上需注意：X11/Wayland 的剪贴板协议要求「选区所有者」进程存活以响应后续
/// 读取请求，所以这些工具写入完成后会 fork 出后台进程继续持有选区。它们**不是**
/// 以「内容已送达」为退出条件的普通命令，等待与关闭 stdin 的语义必须兼容这一点。
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
        copy_with_candidates(text, &linux_clipboard_candidates(), run_command_with_input)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        bail!("clipboard integration is not supported on this platform")
    }
}

/// Linux 剪贴板命令候选，按当前会话类型排序。
///
/// `xclip` / `xsel` 走 X11 选区协议，在纯 Wayland 会话下无法工作
/// （XWayland 也拿不到原生 Wayland 客户端的选区），因此 Wayland 会话下
/// 优先使用 `wl-copy`。
#[cfg(target_os = "linux")]
fn linux_clipboard_candidates() -> Vec<CommandSpec<'static>> {
    linux_clipboard_candidates_for(is_wayland_session())
}

#[cfg(target_os = "linux")]
fn linux_clipboard_candidates_for(is_wayland: bool) -> Vec<CommandSpec<'static>> {
    const XCLIP: CommandSpec<'static> = CommandSpec {
        program: "xclip",
        args: &["-selection", "clipboard"],
    };
    const XSEL: CommandSpec<'static> = CommandSpec {
        program: "xsel",
        args: &["--clipboard"],
    };
    const WL_COPY: CommandSpec<'static> = CommandSpec {
        program: "wl-copy",
        args: &[],
    };

    if is_wayland {
        vec![WL_COPY, XCLIP, XSEL]
    } else {
        vec![XCLIP, XSEL, WL_COPY]
    }
}

/// 判断当前是否为 Wayland 会话。
///
/// `XDG_SESSION_TYPE` 是 systemd/logind 与各桌面环境共同约定的权威来源；
/// 缺失时回退到 `WAYLAND_DISPLAY`。
#[cfg(target_os = "linux")]
fn is_wayland_session() -> bool {
    is_wayland_session_values(
        std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
        std::env::var("WAYLAND_DISPLAY").ok().as_deref(),
    )
}

#[cfg(target_os = "linux")]
fn is_wayland_session_values(session_type: Option<&str>, wayland_display: Option<&str>) -> bool {
    if let Some(session_type) = session_type.filter(|value| !value.trim().is_empty()) {
        return session_type.trim().eq_ignore_ascii_case("wayland");
    }
    wayland_display.is_some_and(|value| !value.trim().is_empty())
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
    run_command_with_input_timeout(program, args, text, CLIPBOARD_COMMAND_TIMEOUT)
}

fn run_command_with_input_timeout(
    program: &str,
    args: &[&str],
    text: &str,
    timeout: std::time::Duration,
) -> Result<()> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::piped())
        // 剪贴板工具不需要 stdout/stderr；置为 null 也避免它们继承我们的终端。
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    prepare_process_group(&mut command);

    let mut child = command
        .spawn()
        .with_context(|| format!("failed to start {program}"))?;
    let mut stdin = match child.stdin.take() {
        Some(stdin) => stdin,
        None => {
            terminate_child_tree(&mut child);
            let _ = child.wait();
            bail!("child process did not expose stdin")
        }
    };

    let write_result = write_with_timeout(&mut stdin, text.as_bytes(), timeout);
    drop(stdin);

    if let Err(err) = write_result {
        terminate_child_tree(&mut child);
        let _ = child.wait();
        return Err(err).with_context(|| format!("failed to write to {program}"));
    }

    let status = match wait_with_timeout(&mut child, program, timeout) {
        Ok(status) => status,
        Err(err) => {
            terminate_child_tree(&mut child);
            let _ = child.wait();
            return Err(err);
        }
    };
    ensure_success(program, status)
}

fn write_with_timeout(
    stdin: &mut std::process::ChildStdin,
    input: &[u8],
    timeout: std::time::Duration,
) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;

        let fd = stdin.as_raw_fd();
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags == -1 {
            return Err(io::Error::last_os_error());
        }
        if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } == -1 {
            return Err(io::Error::last_os_error());
        }

        let result = write_nonblocking_until_deadline(stdin, input, timeout);
        let _ = unsafe { libc::fcntl(fd, libc::F_SETFL, flags) };
        result
    }
    #[cfg(not(unix))]
    {
        let _ = timeout;
        stdin.write_all(input)
    }
}

#[cfg(unix)]
fn write_nonblocking_until_deadline(
    stdin: &mut std::process::ChildStdin,
    input: &[u8],
    timeout: std::time::Duration,
) -> io::Result<()> {
    let deadline = std::time::Instant::now() + timeout;
    let mut written = 0;
    while written < input.len() {
        match stdin.write(&input[written..]) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "clipboard stdin closed before all input was written",
                ));
            }
            Ok(count) => written += count,
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                if std::time::Instant::now() >= deadline {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "clipboard stdin write timed out",
                    ));
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(err) => return Err(err),
        }
    }
    Ok(())
}

/// 有上限地等待子进程退出；超时则结束子进程并报错。
///
/// 没有复用 `providers::common::cli::run_child_with_timeout` 是有意的：
/// 那个函数带进程组、PATH 注入和 stdout/stderr 管道排空，是为 provider 子进程
/// 设计的；剪贴板命令只需要「等到退出」这一件事，套用会引入无关耦合。
/// 两者各约 20 行，重复度低，合并的收益抵不上耦合成本。
fn wait_with_timeout(
    child: &mut Child,
    program: &str,
    timeout: std::time::Duration,
) -> Result<ExitStatus> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    terminate_child_tree(child);
                    let _ = child.wait();
                    bail!(
                        "{program} did not exit within {}ms and was terminated",
                        timeout.as_millis()
                    );
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(err) => {
                return Err(err).with_context(|| format!("failed to wait for {program}"));
            }
        }
    }
}

#[cfg(unix)]
fn prepare_process_group(command: &mut Command) {
    use std::os::unix::process::CommandExt;

    unsafe {
        command.pre_exec(|| {
            if libc::setpgid(0, 0) == -1 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        });
    }
}

#[cfg(not(unix))]
fn prepare_process_group(_command: &mut Command) {}

fn terminate_child_tree(child: &mut Child) {
    #[cfg(unix)]
    {
        if let Ok(pgid) = libc::pid_t::try_from(child.id()) {
            unsafe {
                let _ = libc::kill(-pgid, libc::SIGKILL);
            }
        }
    }
    let _ = child.kill();
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
    system_dark_mode_probe().get_or_refresh(
        false,
        std::time::Duration::from_secs(2),
        detect_system_dark_mode_blocking,
    )
}

/// 在 GPUI 事件循环启动前填充首份主题缓存；平台命令有 500ms 硬超时。
#[allow(dead_code)] // app 启动前 wiring 调用；lib-only 目标不会触达
pub(crate) fn prewarm_system_dark_mode_detection() {
    system_dark_mode_probe().publish(detect_system_dark_mode_blocking());
}

fn system_dark_mode_probe() -> &'static NonBlockingBoolProbe {
    static PROBE: OnceLock<NonBlockingBoolProbe> = OnceLock::new();
    PROBE.get_or_init(NonBlockingBoolProbe::default)
}

fn detect_system_dark_mode_blocking() -> bool {
    #[cfg(target_os = "macos")]
    {
        crate::providers::common::cli::run_command_with_timeout(
            "defaults",
            &["read", "-g", "AppleInterfaceStyle"],
            std::time::Duration::from_millis(500),
        )
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
    if let Ok(output) = crate::providers::common::cli::run_command_with_timeout(
        "gsettings",
        &["get", "org.gnome.desktop.interface", "color-scheme"],
        std::time::Duration::from_millis(500),
    ) {
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
    if let Ok(output) = crate::providers::common::cli::run_command_with_timeout(
        "gsettings",
        &["get", "org.gnome.desktop.interface", "gtk-theme"],
        std::time::Duration::from_millis(500),
    ) {
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
    fn boolean_probe_returns_immediately_while_slow_detection_runs_off_thread() {
        let probe = NonBlockingBoolProbe::default();

        // 闸门式慢检测：检测线程阻塞到测试放行。调用能返回本身就证明它没有
        // 等检测完成；用 sleep + elapsed 上限时，测试线程被抢占超过上限就会
        // 把「没阻塞」误判成阻塞（CI 上出现过同类误报）。
        let (gate_tx, gate_rx) = std::sync::mpsc::channel::<()>();
        let initial = probe.get_or_refresh(false, std::time::Duration::from_secs(30), move || {
            let _ = gate_rx.recv();
            true
        });

        assert!(
            !initial,
            "a still-running detection must return the fallback without blocking"
        );

        gate_tx.send(()).expect("detection gate closed");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        while !probe.get_or_refresh(false, std::time::Duration::from_secs(30), || false) {
            assert!(
                std::time::Instant::now() < deadline,
                "probe result not published"
            );
            std::thread::yield_now();
        }
    }

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
    fn clipboard_command_reaps_after_input_is_closed() {
        let script = "cat >/dev/null; exit 0";

        let started = std::time::Instant::now();
        let result = run_command_with_input("sh", &["-c", script], "payload");
        let elapsed = started.elapsed();

        assert!(result.is_ok(), "expected success, got {result:?}");
        assert!(
            elapsed < CLIPBOARD_COMMAND_TIMEOUT,
            "clipboard copy took {elapsed:?}, indicating the command was not reaped"
        );
    }

    #[test]
    fn clipboard_large_payload_does_not_block_when_reader_consumes_input() {
        let script = "cat >/dev/null; exit 0";
        let large = "x".repeat(256 * 1024);

        let started = std::time::Instant::now();
        let result = run_command_with_input("sh", &["-c", script], &large);
        let elapsed = started.elapsed();

        assert!(result.is_ok(), "expected success, got {result:?}");
        assert!(
            elapsed < CLIPBOARD_COMMAND_TIMEOUT,
            "large clipboard payload took {elapsed:?}"
        );
    }

    #[test]
    fn clipboard_input_timeout_terminates_non_reader() {
        let script = "dd bs=1 count=1 >/dev/null 2>/dev/null; sleep 30";
        let large = "x".repeat(256 * 1024);
        let started = std::time::Instant::now();
        let err = run_command_with_input_timeout(
            "sh",
            &["-c", script],
            &large,
            std::time::Duration::from_millis(100),
        )
        .unwrap_err();
        let err = format!("{err:#}");

        assert!(started.elapsed() < std::time::Duration::from_secs(2));
        assert!(
            err.contains("timed out"),
            "expected stdin timeout, got: {err}"
        );
    }

    #[test]
    fn wait_with_timeout_terminates_process_that_never_exits() {
        let mut command = Command::new("sh");
        command
            .args(["-c", "sleep 30"])
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        prepare_process_group(&mut command);
        let mut child = command.spawn().unwrap();

        let started = std::time::Instant::now();
        let err = wait_with_timeout(&mut child, "sleep", std::time::Duration::from_millis(100))
            .unwrap_err()
            .to_string();

        assert!(started.elapsed() < std::time::Duration::from_secs(5));
        assert!(err.contains("did not exit"), "unexpected error: {err}");
        // 超时路径必须已回收子进程，避免留下僵尸。
        assert!(child.try_wait().unwrap().is_some());
    }

    #[test]
    fn wait_with_timeout_returns_immediately_for_fast_exit() {
        let mut command = Command::new("sh");
        command
            .args(["-c", "exit 0"])
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        prepare_process_group(&mut command);
        let mut child = command.spawn().unwrap();

        let status =
            wait_with_timeout(&mut child, "sh", std::time::Duration::from_secs(10)).unwrap();

        assert!(status.success());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_clipboard_candidates_follow_session_type() {
        let wayland = linux_clipboard_candidates_for(true);
        assert_eq!(
            wayland
                .iter()
                .map(|candidate| candidate.program)
                .collect::<Vec<_>>(),
            vec!["wl-copy", "xclip", "xsel"]
        );

        let x11 = linux_clipboard_candidates_for(false);
        assert_eq!(
            x11.iter()
                .map(|candidate| candidate.program)
                .collect::<Vec<_>>(),
            vec!["xclip", "xsel", "wl-copy"]
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn wayland_session_detection_uses_session_type_then_display_fallback() {
        assert!(is_wayland_session_values(Some("wayland"), None));
        assert!(is_wayland_session_values(Some(" WAYLAND "), None));
        assert!(!is_wayland_session_values(Some("x11"), Some("wayland-0")));
        assert!(is_wayland_session_values(None, Some("wayland-0")));
        assert!(!is_wayland_session_values(Some(""), Some("  ")));
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
