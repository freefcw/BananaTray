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
        .level(level)
        .level_for("wgpu", LevelFilter::Warn)
        .level_for("naga", LevelFilter::Warn)
        .format(|out, message, record| {
            out.finish(format_args!(
                "{} [{}] {:<12} {}",
                Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
                record.level(),
                record.target(),
                message
            ))
        })
        .chain(std::io::stdout())
        .chain(file_dispatch)
        .apply()
        .context("failed to install global logger")?;

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
