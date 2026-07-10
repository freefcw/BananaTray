use std::path::{Path, PathBuf};

use crate::application::{DebugContext, IssueReportContext};
use crate::utils::log_capture::LogCapture;

/// Debug Tab 中需要阻塞式系统调用才能取得的诊断快照。
#[derive(Debug, Clone)]
pub(crate) struct DebugDiagnostics {
    pub(crate) log_file_size: Option<u64>,
    pub(crate) os_info: String,
}

/// 收集 Debug Tab 的阻塞式诊断信息；调用方必须在后台执行器上调用。
pub(crate) fn collect_debug_diagnostics(log_path: Option<PathBuf>) -> DebugDiagnostics {
    let log_file_size = log_path
        .as_ref()
        .and_then(|path| std::fs::metadata(path).ok())
        .map(|metadata| metadata.len());

    DebugDiagnostics {
        log_file_size,
        os_info: crate::platform::system::os_info(),
    }
}

/// 使用缓存诊断和当前内存状态组装 Debug Tab selector 上下文。
pub(crate) fn debug_context_from_diagnostics(
    log_path: Option<PathBuf>,
    diagnostics: Option<&DebugDiagnostics>,
) -> DebugContext {
    DebugContext {
        // 读取实际生效的日志级别（log::max_level 是 source of truth），
        // 而非 RUST_LOG 环境变量（仅为启动时初始配置，运行时不会同步更新）。
        log_level: log::max_level().to_string().to_lowercase(),
        log_path,
        log_file_size: diagnostics.and_then(|snapshot| snapshot.log_file_size),
        os_info: diagnostics
            .map(|snapshot| snapshot.os_info.clone())
            .unwrap_or_else(|| "—".to_string()),
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
    fn debug_diagnostics_collect_log_file_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("debug.log");
        std::fs::write(&log_path, b"abcd").unwrap();

        let diagnostics = collect_debug_diagnostics(Some(log_path.clone()));

        assert_eq!(diagnostics.log_file_size, Some(4));
        assert!(!diagnostics.os_info.is_empty());

        let ctx = debug_context_from_diagnostics(Some(log_path.clone()), Some(&diagnostics));
        assert_eq!(ctx.log_path.as_deref(), Some(log_path.as_path()));
        assert_eq!(ctx.log_file_size, Some(4));
        assert!(!ctx.log_level.is_empty());
        assert!(!ctx.os_info.is_empty());
        assert!(!ctx.locale.is_empty());
        assert!(!ctx.settings_path.is_empty());
        assert!(!ctx.app_version.is_empty());
    }

    #[test]
    fn debug_context_without_diagnostics_uses_non_blocking_placeholders() {
        let log_path = PathBuf::from("/tmp/bananatray-debug.log");

        let ctx = debug_context_from_diagnostics(Some(log_path.clone()), None);

        assert_eq!(ctx.log_path.as_deref(), Some(log_path.as_path()));
        assert_eq!(ctx.log_file_size, None);
        assert_eq!(ctx.os_info, "—");
        assert!(!ctx.log_level.is_empty());
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
