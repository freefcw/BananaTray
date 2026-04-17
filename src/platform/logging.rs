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
use std::fs;
use std::io::{BufRead, BufReader};
#[cfg(feature = "app")]
use std::path::PathBuf;

#[cfg(feature = "app")]
use super::APP_ID_LOWER;

#[cfg(feature = "app")]
#[allow(dead_code)]
pub struct LoggingInit {
    pub log_path: PathBuf,
}

#[cfg(feature = "app")]
#[allow(dead_code)]
pub fn init() -> Result<LoggingInit> {
    let log_path = resolve_log_path()?;
    let level = resolve_log_level();

    let file_dispatch = fern::log_file(&log_path)
        .with_context(|| format!("failed to open log file: {}", log_path.display()))?;

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

/// 读取日志文件末尾的 N 行。
///
/// 目前仅服务于单元测试，因此只在 `cfg(test)` 下编译，避免对生产代码引入
/// 额外的 dead_code suppress。
#[cfg(test)]
fn read_log_tail(path: &std::path::Path, max_lines: usize) -> String {
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
        if ring.len() >= max_lines {
            ring.pop_front();
        }
        ring.push_back(line);
    }

    ring.into_iter().collect::<Vec<_>>().join("\n")
}

/// 读取日志文件中最后 N 条 WARN/ERROR 级别日志行
///
/// 使用 ring buffer 扫描整个文件，只保留 `[WARN]` 或 `[ERROR]` 的行，
/// 返回最后 `max_lines` 条。文件不存在或读取失败时返回空字符串。
pub fn read_last_errors(path: &std::path::Path, max_lines: usize) -> String {
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

        if !line.contains("[WARN]") && !line.contains("[ERROR]") {
            continue;
        }

        if ring.len() >= max_lines {
            ring.pop_front();
        }
        ring.push_back(line);
    }

    ring.into_iter().collect::<Vec<_>>().join("\n")
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

    #[test]
    fn platform_log_base_dir_returns_some() {
        assert!(platform_log_base_dir().is_some());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn platform_log_base_dir_macos_uses_library_logs() {
        let base = platform_log_base_dir().unwrap();
        assert!(
            base.ends_with("Library/Logs"),
            "expected Library/Logs, got {base:?}"
        );
    }

    #[test]
    fn resolve_log_path_env_override() {
        let dir = std::env::temp_dir().join("bananatray_log_test");
        std::env::set_var("BANANATRAY_LOG_DIR", &dir);
        let path = resolve_log_path().unwrap();
        std::env::remove_var("BANANATRAY_LOG_DIR");
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
}
