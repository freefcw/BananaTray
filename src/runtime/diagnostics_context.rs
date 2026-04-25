use std::path::{Path, PathBuf};

use crate::application::{DebugContext, IssueReportContext};
use crate::utils::log_capture::LogCapture;

/// 收集 Debug Tab 所需的运行时信息（含 I/O 副作用）。
pub(crate) fn collect_debug_context(log_path: Option<PathBuf>) -> DebugContext {
    let log_file_size = log_path
        .as_ref()
        .and_then(|path| std::fs::metadata(path).ok())
        .map(|metadata| metadata.len());

    DebugContext {
        // 读取实际生效的日志级别（log::max_level 是 source of truth），
        // 而非 RUST_LOG 环境变量（仅为启动时初始配置，运行时不会同步更新）。
        log_level: log::max_level().to_string().to_lowercase(),
        log_path,
        log_file_size,
        os_info: crate::platform::system::os_info(),
        locale: rust_i18n::locale().to_string(),
        settings_path: crate::settings_store::config_path().display().to_string(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        captured_logs: LogCapture::global().entries(),
    }
}

/// 收集 Issue Report 所需的运行时信息（含日志文件读取）。
pub(crate) fn collect_issue_report_context(log_path: Option<&Path>) -> IssueReportContext {
    let recent_errors = log_path
        .map(|path| crate::platform::logging::read_last_errors(path, 10))
        .unwrap_or_default();

    IssueReportContext {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        git_hash: option_env!("BANANATRAY_GIT_HASH")
            .unwrap_or("unknown")
            .to_string(),
        os_info: crate::platform::system::os_info(),
        locale: rust_i18n::locale().to_string(),
        log_level: log::max_level().to_string().to_lowercase(),
        recent_errors,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_context_collects_log_file_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("debug.log");
        std::fs::write(&log_path, b"abcd").unwrap();

        let ctx = collect_debug_context(Some(log_path.clone()));

        assert_eq!(ctx.log_path.as_deref(), Some(log_path.as_path()));
        assert_eq!(ctx.log_file_size, Some(4));
        assert!(!ctx.log_level.is_empty());
        assert!(!ctx.os_info.is_empty());
        assert!(!ctx.locale.is_empty());
        assert!(!ctx.settings_path.is_empty());
        assert!(!ctx.app_version.is_empty());
    }

    #[test]
    fn issue_report_context_collects_recent_errors_from_log() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("app.log");
        std::fs::write(
            &log_path,
            "2026-01-01 [INFO] ignored\n2026-01-01 [WARN] warning\n2026-01-01 [ERROR] failed\n",
        )
        .unwrap();

        let ctx = collect_issue_report_context(Some(&log_path));

        assert!(ctx.recent_errors.contains("[WARN] warning"));
        assert!(ctx.recent_errors.contains("[ERROR] failed"));
        assert!(!ctx.recent_errors.contains("[INFO] ignored"));
        assert!(!ctx.app_version.is_empty());
        assert!(!ctx.git_hash.is_empty());
        assert!(!ctx.os_info.is_empty());
        assert!(!ctx.locale.is_empty());
        assert!(!ctx.log_level.is_empty());
    }
}
