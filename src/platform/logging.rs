#[cfg(feature = "app")]
use anyhow::{Context, Result};
#[cfg(feature = "app")]
use chrono::Local;
#[cfg(feature = "app")]
use log::LevelFilter;
#[cfg(feature = "app")]
use std::backtrace::Backtrace;
#[cfg(feature = "app")]
use std::env;
#[cfg(any(feature = "app", test))]
use std::fs;
#[cfg(feature = "app")]
use std::io::Write;
#[cfg(any(feature = "app", test))]
use std::io::{BufRead, BufReader};
#[cfg(feature = "app")]
use std::path::{Path, PathBuf};
#[cfg(feature = "app")]
use std::sync::Mutex;

#[cfg(feature = "app")]
use super::APP_ID_LOWER;
#[cfg(feature = "app")]
use crate::models::LoggingSettings;

#[cfg(feature = "app")]
#[allow(dead_code)]
pub struct LoggingInit {
    pub log_path: PathBuf,
}

/// 日志轮转 / 清理阈值（运行时不可变，启动时从 settings 解析一次）。
///
/// `max_bytes == 0` 表示禁用轮转（行为退化为原来的单文件 append）。
#[cfg(feature = "app")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LogLimits {
    max_bytes: u64,
    max_files: usize,
}

/// 从 `LoggingSettings` 解析运行时阈值，非法值回退到模型层默认。
///
/// `max_files` 下限为 1（至少保留一份轮转）：0 会让轮转时无法删除旧文件、
/// 只会反复重命名活跃文件，语义未定义，因此强制抬到 1。
#[cfg(feature = "app")]
fn resolve_log_limits(settings: &LoggingSettings) -> LogLimits {
    LogLimits {
        max_bytes: settings.max_bytes,
        max_files: settings.max_files.max(1),
    }
}

/// 按 size-based 轮转的文件写入器。
///
/// - 写入累加内存计数器 `bytes_written`；达到 `max_bytes` 时在 `flush()` 边界轮转
///   （fern 对 `Writer` 在每条日志记录后调用 `flush`，因此轮转必然发生在完整记录后）。
/// - 轮转执行 rename 链：删除最老的 `.{max_files}`，其余 `.{n}` -> `.{n+1}`，
///   当前 `log_path` -> `log_path.1`，最后重新打开新的活跃文件。
/// - 内部 `Mutex` 包裹，`lock` 用 `into_inner` 抗 poison（panic hook 在 panic 时写日志）。
#[cfg(feature = "app")]
struct RotatingFileWriter {
    inner: Mutex<Inner>,
}

#[cfg(feature = "app")]
struct Inner {
    path: PathBuf,
    file: fs::File,
    bytes_written: u64,
    max_bytes: u64,
    max_files: usize,
}

#[cfg(feature = "app")]
impl RotatingFileWriter {
    fn open(path: PathBuf, limits: LogLimits) -> std::io::Result<Self> {
        let file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        // 以当前文件大小初始化计数器，避免对已有大文件的首条日志误触发轮转
        let bytes_written = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        Ok(Self {
            inner: Mutex::new(Inner {
                path,
                file,
                bytes_written,
                max_bytes: limits.max_bytes,
                max_files: limits.max_files,
            }),
        })
    }

    /// 执行 rename 链轮转并重新打开活跃文件。失败时尽力重开活跃文件以继续写入。
    fn rotate(&self, inner: &mut Inner) {
        // flush 已由调用方完成
        let _ = inner.file.flush();
        // 从最老开始删除：.{max_files}（如 max_files=4 则删 .4）
        for n in (1..=inner.max_files).rev() {
            let cur = rotated_path_for(&inner.path, n);
            if n == inner.max_files {
                // 最老一份：删除
                let _ = fs::remove_file(&cur);
            } else if cur.exists() {
                // .n -> .{n+1}
                let next = rotated_path_for(&inner.path, n + 1);
                let _ = fs::rename(&cur, &next);
            }
        }
        // 当前活跃 -> .1
        let first = rotated_path_for(&inner.path, 1);
        let _ = fs::rename(&inner.path, &first);
        // 重开新活跃文件
        match fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&inner.path)
        {
            Ok(f) => {
                inner.file = f;
                inner.bytes_written = 0;
            }
            Err(e) => {
                // 重开失败极罕见；记录到 stderr 避免在 logger 内部递归调用 log!
                eprintln!("bananatray: failed to reopen log file after rotation: {e}");
            }
        }
    }
}

#[cfg(feature = "app")]
impl std::io::Write for RotatingFileWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let n = inner.file.write(buf)?;
        inner.bytes_written += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.file.flush()?;
        // 仅在跨过阈值时轮转一次；轮转后 bytes_written 清零
        if inner.max_bytes > 0 && inner.bytes_written >= inner.max_bytes {
            self.rotate(&mut inner);
        }
        Ok(())
    }
}

#[cfg(feature = "app")]
#[allow(dead_code)]
pub fn init(settings: &LoggingSettings) -> Result<LoggingInit> {
    let log_path = resolve_log_path()?;
    let level = resolve_log_level();
    let limits = resolve_log_limits(settings);

    // 启动清理：删除超出 max_files 的残留轮转文件。
    // 轮转本身在运行时已硬性封顶文件数，此处理为配置变更后收敛。
    cleanup_old_logs(&log_path, limits.max_files);

    let writer = RotatingFileWriter::open(log_path.clone(), limits)
        .with_context(|| format!("failed to open log file: {}", log_path.display()))?;
    let file_dispatch: Box<dyn std::io::Write + Send> = Box::new(writer);

    fern::Dispatch::new()
        // Dispatch 层不过滤，依赖 log::set_max_level 控制级别，
        // 以支持运行时通过 Debug Tab 动态调整日志级别。
        .level(LevelFilter::Trace)
        .level_for("wgpu", LevelFilter::Warn)
        .level_for("naga", LevelFilter::Warn)
        // GPUI 框架内部 display link 回调在窗口关闭后仍可能触发，产生无害的
        // "window not found" ERROR 日志（空 target）。过滤掉这类噪音。
        .filter(|metadata| {
            // 空 target 的 ERROR 来自 GPUI 内部（registry crate 路径无 "crates/" 前缀
            // 导致 target 为空），降级过滤
            !(metadata.target().is_empty() && metadata.level() == log::Level::Error)
        })
        .format(|out, message, record| {
            let formatted = format!(
                "{} [{}] {:<12} {}",
                Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
                record.level(),
                record.target(),
                message
            );
            // 同步写入 Debug Tab 的日志捕获器（如果已启用）
            crate::utils::log_capture::LogCapture::global().try_push(
                record.level(),
                record.target(),
                &message.to_string(),
            );
            out.finish(format_args!("{}", formatted))
        })
        .chain(std::io::stdout())
        .chain(file_dispatch)
        .apply()
        .context("failed to install global logger")?;

    // 通过全局 max level 控制实际过滤（运行时可动态调整）
    log::set_max_level(level);

    install_panic_hook();

    Ok(LoggingInit { log_path })
}

#[cfg(feature = "app")]
fn resolve_log_path() -> Result<PathBuf> {
    if let Ok(dir) = env::var("BANANATRAY_LOG_DIR") {
        let path = PathBuf::from(dir);
        fs::create_dir_all(&path)
            .with_context(|| format!("failed to create log directory: {}", path.display()))?;
        return Ok(path.join(format!("{APP_ID_LOWER}.log")));
    }

    let base_dir = platform_log_base_dir()
        .or_else(|| env::current_dir().ok().map(|dir| dir.join("logs")))
        .context("failed to resolve log directory")?;

    let log_dir = base_dir.join(APP_ID_LOWER);
    fs::create_dir_all(&log_dir)
        .with_context(|| format!("failed to create log directory: {}", log_dir.display()))?;

    Ok(log_dir.join(format!("{APP_ID_LOWER}.log")))
}

/// 删除超出 `max_files` 的残留轮转文件（`<base>.log.{n}` where n > max_files）。
///
/// 运行时轮转已硬性封顶文件数；此函数仅为配置变更后的收敛清理，
/// 例如用户调小 `max_files` 后启动时删除多余历史文件。
#[cfg(feature = "app")]
fn cleanup_old_logs(active_path: &Path, max_files: usize) {
    let Some(dir) = active_path.parent() else {
        return;
    };
    let Some(stem) = active_path
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
    else {
        return;
    };

    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            continue;
        };
        let prefix = format!("{stem}.");
        // 仅匹配 `<stem>.<n>` 形态的轮转文件
        let Some(suffix) = name.strip_prefix(&prefix) else {
            continue;
        };
        let Ok(index) = suffix.parse::<usize>() else {
            continue;
        };
        if index > max_files {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// 返回符合各平台规范的日志根目录：
/// - macOS: `~/Library/Logs`
/// - Linux/其他: `$XDG_STATE_HOME`（默认 `~/.local/state`），fallback 到 `data_local_dir`
#[cfg(feature = "app")]
fn platform_log_base_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        dirs::home_dir().map(|h| h.join("Library/Logs"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        dirs::state_dir().or_else(dirs::data_local_dir)
    }
}
#[cfg(feature = "app")]
fn resolve_log_level() -> LevelFilter {
    match env::var("RUST_LOG") {
        Ok(value) => match value.to_ascii_lowercase().as_str() {
            "trace" => LevelFilter::Trace,
            "debug" => LevelFilter::Debug,
            "warn" => LevelFilter::Warn,
            "error" => LevelFilter::Error,
            "off" => LevelFilter::Off,
            _ => LevelFilter::Info,
        },
        Err(_) => LevelFilter::Info,
    }
}

/// 扫描文件，用 ring buffer 保留最后 `max_lines` 条满足 `filter` 的行。
/// 文件不存在或读取失败时返回空字符串。
#[cfg(test)]
fn read_last_filtered_lines(
    path: &std::path::Path,
    max_lines: usize,
    mut filter: impl FnMut(&str) -> bool,
) -> String {
    let file = match fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return String::new(),
    };

    let reader = BufReader::new(file);
    let mut ring: std::collections::VecDeque<String> =
        std::collections::VecDeque::with_capacity(max_lines);

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        if !filter(&line) {
            continue;
        }
        if ring.len() >= max_lines {
            ring.pop_front();
        }
        ring.push_back(line);
    }

    ring.into_iter().collect::<Vec<_>>().join("\n")
}

/// 读取日志文件末尾的 N 行。
#[cfg(test)]
fn read_log_tail(path: &std::path::Path, max_lines: usize) -> String {
    read_last_filtered_lines(path, max_lines, |_| true)
}

/// 读取活跃日志文件及其轮转文件中最后 N 条 WARN/ERROR 级别日志行。
///
/// 轮转刚发生时活跃文件几乎为空，最近的错误在 `.1`；因此必须跨文件聚合。
///
/// 读取顺序：从最老的轮转文件开始，`.N` -> ... -> `.1` -> 活跃文件（从旧到新），
/// 用容量为 `max_lines` 的 ring buffer 保留最后写入的（即最新的）N 条，
/// 这样即使总量超过 N 也不会丢弃活跃文件中的最新错误。
#[cfg(any(feature = "app", test))]
pub fn read_last_errors(path: &std::path::Path, max_lines: usize) -> String {
    if max_lines == 0 {
        return String::new();
    }

    let mut ring: std::collections::VecDeque<String> =
        std::collections::VecDeque::with_capacity(max_lines);
    let is_error_line = |line: &str| line.contains("[WARN]") || line.contains("[ERROR]");

    // 1. 找到最大的轮转序号（顺序探测，遇到不存在的即停）
    let mut max_index = 0;
    let mut probe = 1;
    loop {
        let candidate = rotated_path_for(path, probe);
        if !candidate.exists() {
            break;
        }
        max_index = probe;
        probe += 1;
        // 上限保护，避免异常目录下无限探测
        if probe > 9999 {
            break;
        }
    }

    // 2. 从最老到最新依次读入 ring buffer
    for index in (1..=max_index).rev() {
        append_filtered_lines(
            &rotated_path_for(path, index),
            max_lines,
            is_error_line,
            &mut ring,
        );
    }
    append_filtered_lines(path, max_lines, is_error_line, &mut ring);

    ring.into_iter().collect::<Vec<_>>().join("\n")
}

/// 将 `path` 中满足 `filter` 的行追加进 `ring`，保持 `ring` 容量不超过 `max_lines`
///（超出则从最老端弹出）。文件不存在或读取失败时静默跳过。
///
/// 调用方保证从旧到新的顺序读入文件，这样 ring buffer 保留的始终是最新 N 条。
#[cfg(any(feature = "app", test))]
fn append_filtered_lines(
    path: &std::path::Path,
    max_lines: usize,
    mut filter: impl FnMut(&str) -> bool,
    ring: &mut std::collections::VecDeque<String>,
) {
    let Ok(file) = fs::File::open(path) else {
        return;
    };
    let reader = BufReader::new(file);
    for line in reader.lines() {
        let Ok(line) = line else { continue };
        if !filter(&line) {
            continue;
        }
        if ring.len() >= max_lines {
            ring.pop_front();
        }
        ring.push_back(line);
    }
}

/// 返回 `base` 的第 `index` 个轮转文件路径（`base` + `.{index}`）。
/// lib/test 路径无 `Path` 导入时使用本地拼装。
#[cfg(any(feature = "app", test))]
fn rotated_path_for(base: &std::path::Path, index: usize) -> std::path::PathBuf {
    let mut name = base
        .file_name()
        .map(|s| s.to_os_string())
        .unwrap_or_default();
    name.push(format!(".{index}"));
    base.with_file_name(name)
}

#[cfg(feature = "app")]
fn install_panic_hook() {
    std::panic::set_hook(Box::new(|panic_info| {
        let location = panic_info
            .location()
            .map(|loc| format!("{}:{}", loc.file(), loc.line()))
            .unwrap_or_else(|| "unknown location".to_string());

        let payload = if let Some(msg) = panic_info.payload().downcast_ref::<&str>() {
            (*msg).to_string()
        } else if let Some(msg) = panic_info.payload().downcast_ref::<String>() {
            msg.clone()
        } else {
            "unknown panic payload".to_string()
        };

        log::error!(
            target: "bananatray::panic",
            "panic at {}: {}\n{}",
            location,
            payload,
            Backtrace::force_capture()
        );
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "app")]
    use crate::utils::test_support::env_lock;

    #[cfg(feature = "app")]
    #[test]
    fn platform_log_base_dir_returns_some() {
        assert!(platform_log_base_dir().is_some());
    }

    #[cfg(feature = "app")]
    #[cfg(target_os = "macos")]
    #[test]
    fn platform_log_base_dir_macos_uses_library_logs() {
        let base = platform_log_base_dir().unwrap();
        assert!(
            base.ends_with("Library/Logs"),
            "expected Library/Logs, got {base:?}"
        );
    }

    #[cfg(feature = "app")]
    #[test]
    fn resolve_log_path_env_override() {
        let _guard = env_lock().lock().unwrap();
        let dir = std::env::temp_dir().join("bananatray_log_test");
        unsafe { std::env::set_var("BANANATRAY_LOG_DIR", &dir) };
        let path = resolve_log_path().unwrap();
        unsafe { std::env::remove_var("BANANATRAY_LOG_DIR") };
        assert_eq!(path, dir.join("bananatray.log"));
    }

    #[test]
    fn read_log_tail_nonexistent_returns_empty() {
        let result = read_log_tail(std::path::Path::new("/nonexistent/path/log.txt"), 10);
        assert!(result.is_empty());
    }

    #[test]
    fn read_log_tail_fewer_lines_than_max() {
        let dir = std::env::temp_dir().join("bananatray_tail_test_fewer");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.log");
        fs::write(&path, "line1\nline2\nline3\n").unwrap();

        let result = read_log_tail(&path, 10);
        assert_eq!(result, "line1\nline2\nline3");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_log_tail_more_lines_than_max() {
        let dir = std::env::temp_dir().join("bananatray_tail_test_more");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.log");

        let lines: Vec<String> = (0..20).map(|i| format!("line {}", i)).collect();
        fs::write(&path, lines.join("\n")).unwrap();

        let result = read_log_tail(&path, 5);
        let tail_lines: Vec<&str> = result.lines().collect();
        assert_eq!(tail_lines.len(), 5);
        assert_eq!(tail_lines[0], "line 15");
        assert_eq!(tail_lines[4], "line 19");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_log_tail_empty_file() {
        let dir = std::env::temp_dir().join("bananatray_tail_test_empty");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("empty.log");
        fs::write(&path, "").unwrap();

        let result = read_log_tail(&path, 10);
        assert!(result.is_empty());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_last_errors_filters_by_level() {
        let dir = std::env::temp_dir().join("bananatray_errors_test_level");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.log");

        let content = "2026-04-11 10:00:00.000 [INFO] app        normal info\n\
             2026-04-11 10:00:01.000 [WARN] providers  slow response\n\
             2026-04-11 10:00:02.000 [DEBUG] refresh   tick\n\
             2026-04-11 10:00:03.000 [ERROR] providers fetch failed\n";
        fs::write(&path, content).unwrap();

        let result = read_last_errors(&path, 100);
        assert!(result.contains("[WARN]"));
        assert!(result.contains("[ERROR]"));
        assert!(!result.contains("[INFO]"));
        assert!(!result.contains("[DEBUG]"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_last_errors_nonexistent_returns_empty() {
        let result = read_last_errors(std::path::Path::new("/nonexistent/path/log.txt"), 10);
        assert!(result.is_empty());
    }

    #[test]
    fn read_last_errors_respects_max_lines() {
        let dir = std::env::temp_dir().join("bananatray_errors_test_max");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.log");

        let lines: Vec<String> = (0..20)
            .map(|i| format!("2026-04-11 10:00:{i:02}.000 [ERROR] test      error {i}"))
            .collect();
        fs::write(&path, lines.join("\n")).unwrap();

        let result = read_last_errors(&path, 3);
        let result_lines: Vec<&str> = result.lines().collect();
        assert_eq!(result_lines.len(), 3);
        // 保留最后 3 条
        assert!(result_lines[0].contains("error 17"));
        assert!(result_lines[2].contains("error 19"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(feature = "app")]
    #[test]
    fn resolve_log_limits_clamps_max_files_to_one() {
        let s = LoggingSettings {
            max_bytes: 100,
            max_files: 0,
        };
        assert_eq!(
            resolve_log_limits(&s),
            LogLimits {
                max_bytes: 100,
                max_files: 1
            }
        );
    }

    #[cfg(feature = "app")]
    #[test]
    fn rotating_writer_max_files_one_keeps_single_rotation() {
        let dir = std::env::temp_dir().join("bananatray_rotate_test_mf1");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bananatray.log");

        let limits = LogLimits {
            max_bytes: 4,
            max_files: 1,
        };
        let mut writer = RotatingFileWriter::open(path.clone(), limits).unwrap();
        for i in 0..6 {
            writer.write_all(format!("block{i}").as_bytes()).unwrap();
            writer.flush().unwrap();
        }

        // 仅保留 1 份轮转：.1 存在，.2 不应存在
        assert!(rotated_path_for(&path, 1).exists());
        assert!(
            !rotated_path_for(&path, 2).exists(),
            ".2 must not exist when max_files=1"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(feature = "app")]
    #[test]
    fn rotating_writer_initializes_counter_from_existing_file_size() {
        // 验证：对已存在的超大文件，首条日志的 flush 会立即轮转
        let dir = std::env::temp_dir().join("bananatray_rotate_test_preexisting");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bananatray.log");
        // 预置一个超过阈值的大文件
        fs::write(&path, "X".repeat(100)).unwrap();

        let limits = LogLimits {
            max_bytes: 10,
            max_files: 2,
        };
        let mut writer = RotatingFileWriter::open(path.clone(), limits).unwrap();
        writer.write_all(b"new").unwrap();
        writer.flush().unwrap();

        // 以 append 模式打开，"new" 追加到原 100 字节后；首条 flush 因总量超阈值轮转：
        // .1 = 原文件 + "new"，活跃文件被重开为空
        assert_eq!(
            fs::read_to_string(rotated_path_for(&path, 1)).unwrap(),
            format!("{}new", "X".repeat(100))
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "");

        let _ = fs::remove_dir_all(&dir);
    }

    // ── 轮转 / 清理 / 跨文件读取测试 ───────────────────────────────

    #[cfg(feature = "app")]
    #[test]
    fn resolve_log_limits_uses_settings() {
        let s = LoggingSettings {
            max_bytes: 1024,
            max_files: 3,
        };
        assert_eq!(
            resolve_log_limits(&s),
            LogLimits {
                max_bytes: 1024,
                max_files: 3
            }
        );
    }

    #[cfg(feature = "app")]
    #[test]
    fn rotated_path_for_appends_index() {
        let base = std::path::Path::new("/tmp/bananatray.log");
        assert_eq!(
            rotated_path_for(base, 1),
            std::path::PathBuf::from("/tmp/bananatray.log.1")
        );
        assert_eq!(
            rotated_path_for(base, 7),
            std::path::PathBuf::from("/tmp/bananatray.log.7")
        );
    }

    #[cfg(feature = "app")]
    #[test]
    fn rotating_writer_rotates_on_flush_when_over_threshold() {
        let dir = std::env::temp_dir().join("bananatray_rotate_test_basic");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bananatray.log");

        let limits = LogLimits {
            max_bytes: 10,
            max_files: 2,
        };
        let mut writer = RotatingFileWriter::open(path.clone(), limits).unwrap();
        // 写入超过 max_bytes，flush 后应触发轮转
        writer.write_all(b"0123456789AB").unwrap();
        writer.flush().unwrap();

        // 轮转后：活跃文件被重开且为空，原内容移到 .1
        assert!(path.exists());
        let active = fs::read_to_string(&path).unwrap();
        assert!(
            active.is_empty(),
            "active file should be empty after rotation, got: {active:?}"
        );
        let rotated = fs::read_to_string(rotated_path_for(&path, 1)).unwrap();
        assert_eq!(rotated, "0123456789AB");

        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(feature = "app")]
    #[test]
    fn rotating_writer_enforces_max_files_cap() {
        let dir = std::env::temp_dir().join("bananatray_rotate_test_cap");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bananatray.log");

        // max_files=2 表示保留 2 份轮转（.1 和 .2），活跃文件不计入
        let limits = LogLimits {
            max_bytes: 4,
            max_files: 2,
        };
        let mut writer = RotatingFileWriter::open(path.clone(), limits).unwrap();

        // 连续写多轮，足够触发超过 max_files+1 次轮转
        for i in 0..10 {
            writer.write_all(format!("block{i}").as_bytes()).unwrap();
            writer.flush().unwrap();
        }

        // 最老一份被删除：不应存在 .3 及以上
        assert!(
            !rotated_path_for(&path, 3).exists(),
            ".3 should have been deleted"
        );
        // .1 和 .2 应存在
        assert!(rotated_path_for(&path, 1).exists());
        assert!(rotated_path_for(&path, 2).exists());

        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(feature = "app")]
    #[test]
    fn rotating_writer_disabled_when_max_bytes_zero() {
        let dir = std::env::temp_dir().join("bananatray_rotate_test_disabled");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bananatray.log");

        let limits = LogLimits {
            max_bytes: 0,
            max_files: 2,
        };
        let mut writer = RotatingFileWriter::open(path.clone(), limits).unwrap();
        writer.write_all(b"0123456789AB").unwrap();
        writer.flush().unwrap();

        // 不轮转：内容仍在活跃文件，无 .1
        assert_eq!(fs::read_to_string(&path).unwrap(), "0123456789AB");
        assert!(!rotated_path_for(&path, 1).exists());

        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(feature = "app")]
    #[test]
    fn cleanup_old_logs_removes_excess_rotations() {
        let dir = std::env::temp_dir().join("bananatray_cleanup_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bananatray.log");

        // 预置：.1 .2 .3 .4 .5，max_files=2 应保留 .1 .2，删 .3 .4 .5
        for i in 1..=5 {
            fs::write(rotated_path_for(&path, i), format!("old{i}")).unwrap();
        }
        fs::write(&path, "active").unwrap();

        cleanup_old_logs(&path, 2);

        assert!(rotated_path_for(&path, 1).exists());
        assert!(rotated_path_for(&path, 2).exists());
        assert!(!rotated_path_for(&path, 3).exists());
        assert!(!rotated_path_for(&path, 4).exists());
        assert!(!rotated_path_for(&path, 5).exists());
        // 活跃文件不受影响
        assert_eq!(fs::read_to_string(&path).unwrap(), "active");

        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(feature = "app")]
    #[test]
    fn read_last_errors_aggregates_across_rotation_files() {
        let dir = std::env::temp_dir().join("bananatray_errors_cross_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bananatray.log");

        // 活跃文件只有 1 条错误，.1 有 3 条，.2 有 3 条
        fs::write(&path, "2026-04-11 10:00:00.000 [ERROR] active   newest\n").unwrap();
        fs::write(
            rotated_path_for(&path, 1),
            "2026-04-11 09:00:00.000 [INFO]  r1       noise\n\
             2026-04-11 09:00:01.000 [ERROR] r1       e1\n\
             2026-04-11 09:00:02.000 [ERROR] r1       e2\n\
             2026-04-11 09:00:03.000 [ERROR] r1       e3\n",
        )
        .unwrap();
        fs::write(
            rotated_path_for(&path, 2),
            "2026-04-11 08:00:00.000 [ERROR] r2       old1\n\
             2026-04-11 08:00:01.000 [ERROR] r2       old2\n",
        )
        .unwrap();

        // 要 5 条：.2 最老，只保留 old2；.1 三条；活跃一条
        // 从旧到新输出：old2, e1, e2, e3, newest
        let result = read_last_errors(&path, 5);
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines.len(), 5, "expected 5 lines, got {lines:?}");
        // 从旧到新排列
        assert!(lines[0].contains("old2"));
        assert!(lines[1].contains("e1"));
        assert!(lines[2].contains("e2"));
        assert!(lines[3].contains("e3"));
        assert!(lines[4].contains("newest"));
        // 被淘汰的是最老的 old1，以及 INFO 噪声
        assert!(!result.contains("old1"));
        assert!(!result.contains("noise"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(feature = "app")]
    #[test]
    fn read_last_errors_stops_when_active_file_has_enough() {
        let dir = std::env::temp_dir().join("bananatray_errors_enough_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bananatray.log");

        // 活跃文件已有 5 条错误，足够；.1 不应被读
        fs::write(
            &path,
            (0..5)
                .map(|i| format!("2026-04-11 10:00:{i:02}.000 [ERROR] active e{i}"))
                .collect::<Vec<_>>()
                .join("\n")
                + "\n",
        )
        .unwrap();
        fs::write(
            rotated_path_for(&path, 1),
            "2026-04-11 09:00:00.000 [ERROR] should_not_appear\n",
        )
        .unwrap();

        let result = read_last_errors(&path, 5);
        assert!(!result.contains("should_not_appear"));
        assert_eq!(result.lines().count(), 5);

        let _ = fs::remove_dir_all(&dir);
    }
}
