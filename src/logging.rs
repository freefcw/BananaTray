use anyhow::{Context, Result};
use chrono::Local;
use log::LevelFilter;
use std::backtrace::Backtrace;
use std::env;
use std::fs;
use std::path::PathBuf;

pub struct LoggingInit {
    pub log_path: PathBuf,
}

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

fn resolve_log_path() -> Result<PathBuf> {
    if let Ok(dir) = env::var("BANANATRAY_LOG_DIR") {
        let path = PathBuf::from(dir);
        fs::create_dir_all(&path)
            .with_context(|| format!("failed to create log directory: {}", path.display()))?;
        return Ok(path.join("bananatray.log"));
    }

    let base_dir = dirs::state_dir()
        .or_else(dirs::data_local_dir)
        .or_else(|| env::current_dir().ok().map(|dir| dir.join("logs")))
        .context("failed to resolve log directory")?;

    let log_dir = base_dir.join("bananatray");
    fs::create_dir_all(&log_dir)
        .with_context(|| format!("failed to create log directory: {}", log_dir.display()))?;

    Ok(log_dir.join("bananatray.log"))
}

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
