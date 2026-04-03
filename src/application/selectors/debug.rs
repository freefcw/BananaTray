//! Debug Tab 的 ViewModel 选择器
//!
//! 将 AppSession + DebugContext 转换为 Debug Tab 所需的 ViewState。
//! selector 是纯函数：`(AppSession, DebugContext) → DebugTabViewState`
//! 所有 I/O 和环境变量读取都在 DebugContext 构造时完成。

use crate::app_state::AppSession;
use crate::models::{ConnectionStatus, ProviderKind};
use crate::utils::log_capture::LogEntry;
use std::path::PathBuf;

// ============================================================================
// 运行时上下文（在调用 selector 之前构造，收集所有副作用数据）
// ============================================================================

/// 收集 Debug Tab 所需的运行时信息（I/O、环境变量等）。
/// Selector 不再直接读取这些副作用来源，而是从此结构中获取。
#[derive(Debug, Clone)]
pub struct DebugContext {
    /// 当前日志级别 (RUST_LOG)
    pub log_level: String,
    /// 日志文件路径
    pub log_path: Option<PathBuf>,
    /// 日志文件大小（若文件存在）
    pub log_file_size: Option<u64>,
    /// 操作系统版本信息
    pub os_info: String,
    /// 当前 locale
    pub locale: String,
    /// 配置文件路径
    pub settings_path: String,
    /// 应用版本号
    pub app_version: String,
    /// 调试控制台捕获的日志条目（从 LogCapture 读取）
    pub captured_logs: Vec<LogEntry>,
}

impl DebugContext {
    /// 从系统收集运行时信息（含 I/O 副作用）
    pub fn collect(log_path: Option<PathBuf>) -> Self {
        use crate::utils::log_capture::LogCapture;

        let log_file_size = log_path
            .as_ref()
            .and_then(|p| std::fs::metadata(p).ok())
            .map(|m| m.len());

        Self {
            log_level: std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
            log_path,
            log_file_size,
            os_info: crate::utils::platform::os_info(),
            locale: rust_i18n::locale().to_string(),
            settings_path: crate::settings_store::config_path().display().to_string(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            captured_logs: LogCapture::global().entries(),
        }
    }
}

// ============================================================================
// ViewState 类型定义
// ============================================================================

/// Debug Tab 整体 ViewState
#[derive(Debug, Clone)]
pub struct DebugTabViewState {
    pub log: LogViewState,
    pub providers: Vec<ProviderDiagnosticItem>,
    pub environment: EnvironmentViewState,
    pub console: DebugConsoleViewState,
}

/// 调试控制台区域
#[derive(Debug, Clone)]
pub struct DebugConsoleViewState {
    /// 可选择的 Provider 列表（已启用的）
    pub available_providers: Vec<(ProviderKind, String)>,
    /// 当前选中的 Provider
    pub selected_provider: Option<ProviderKind>,
    /// 是否正在调试刷新中
    pub refresh_active: bool,
    /// 捕获的日志条目
    pub log_entries: Vec<CapturedLogEntry>,
    /// 日志条目数量（用于显示计数）
    pub log_count: usize,
}

/// 单条捕获的日志（用于 UI 渲染）
#[derive(Debug, Clone)]
pub struct CapturedLogEntry {
    pub timestamp: String,
    pub level: String,
    pub level_color: LogLevelColor,
    pub target: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy)]
pub enum LogLevelColor {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

/// 日志区域
#[derive(Debug, Clone)]
pub struct LogViewState {
    pub current_level: String,
    pub log_path: Option<String>,
    pub log_file_size: Option<String>,
}

/// 单个 Provider 的诊断信息
#[derive(Debug, Clone)]
#[allow(dead_code)] // kind 用于测试断言，error_message 用于 debug_info_text
pub struct ProviderDiagnosticItem {
    pub kind: ProviderKind,
    pub display_name: String,
    pub icon: String,
    pub source: String,
    pub status_text: String,
    pub status_dot: ProviderDiagnosticStatus,
    pub quota_count: usize,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderDiagnosticStatus {
    Connected,
    Refreshing,
    Error,
    Disconnected,
    Disabled,
}

/// 环境信息
#[derive(Debug, Clone)]
pub struct EnvironmentViewState {
    pub app_version: String,
    pub os_info: String,
    pub log_level: String,
    pub locale: String,
    pub settings_path: String,
    pub log_path: String,
    pub providers_summary: String,
    pub refresh_interval: String,
}

// ============================================================================
// Selector 函数（纯函数，无 I/O）
// ============================================================================

/// 构建 Debug Tab 的完整 ViewState
///
/// 纯函数：所有运行时数据通过 `ctx` 注入
pub fn debug_tab_view_state(session: &AppSession, ctx: &DebugContext) -> DebugTabViewState {
    DebugTabViewState {
        log: build_log_view_state(ctx),
        providers: build_provider_diagnostics(session),
        environment: build_environment_view_state(session, ctx),
        console: build_console_view_state(session, ctx),
    }
}

fn build_log_view_state(ctx: &DebugContext) -> LogViewState {
    LogViewState {
        current_level: ctx.log_level.clone(),
        log_path: ctx.log_path.as_ref().map(|p| p.display().to_string()),
        log_file_size: ctx
            .log_file_size
            .map(crate::utils::platform::format_file_size),
    }
}

fn build_provider_diagnostics(session: &AppSession) -> Vec<ProviderDiagnosticItem> {
    session
        .settings
        .ordered_providers()
        .into_iter()
        .filter_map(|kind| {
            session.provider_store.find(kind).map(|provider| {
                let is_enabled = session.settings.is_provider_enabled(kind);

                let (status_text, status_dot) = if !is_enabled {
                    ("Disabled".to_string(), ProviderDiagnosticStatus::Disabled)
                } else {
                    match provider.connection {
                        ConnectionStatus::Connected => {
                            let time_text = provider.format_last_updated();
                            (
                                format!("Connected · {}", time_text),
                                ProviderDiagnosticStatus::Connected,
                            )
                        }
                        ConnectionStatus::Refreshing => (
                            "Refreshing…".to_string(),
                            ProviderDiagnosticStatus::Refreshing,
                        ),
                        ConnectionStatus::Error => {
                            let msg = provider.error_message.as_deref().unwrap_or("unknown error");
                            (format!("Error · {}", msg), ProviderDiagnosticStatus::Error)
                        }
                        ConnectionStatus::Disconnected => {
                            let msg = provider
                                .error_message
                                .as_deref()
                                .map(|m| format!("Disconnected · {}", m))
                                .unwrap_or_else(|| "Disconnected".to_string());
                            (msg, ProviderDiagnosticStatus::Disconnected)
                        }
                    }
                };

                let quota_count = if is_enabled { provider.quotas.len() } else { 0 };

                ProviderDiagnosticItem {
                    kind,
                    display_name: provider.display_name().to_string(),
                    icon: provider.icon_asset().to_string(),
                    source: provider.source_label().to_string(),
                    status_text,
                    status_dot,
                    quota_count,
                    error_message: if is_enabled {
                        provider.error_message.clone()
                    } else {
                        None
                    },
                }
            })
        })
        .collect()
}

fn build_environment_view_state(session: &AppSession, ctx: &DebugContext) -> EnvironmentViewState {
    let enabled_count = ProviderKind::all()
        .iter()
        .filter(|k| session.settings.is_provider_enabled(**k))
        .count();
    let total_count = ProviderKind::all().len();

    let refresh_text = if session.settings.refresh_interval_mins == 0 {
        "Manual".to_string()
    } else {
        format!("{} min", session.settings.refresh_interval_mins)
    };

    EnvironmentViewState {
        app_version: ctx.app_version.clone(),
        os_info: ctx.os_info.clone(),
        log_level: ctx.log_level.clone(),
        locale: ctx.locale.clone(),
        settings_path: ctx.settings_path.clone(),
        log_path: ctx
            .log_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "—".to_string()),
        providers_summary: format!("{} / {} enabled", enabled_count, total_count),
        refresh_interval: refresh_text,
    }
}

/// 将环境信息和 Provider 诊断收集为一个可复制的纯文本报告
pub fn build_debug_info_text(state: &DebugTabViewState) -> String {
    let env = &state.environment;
    let mut lines = vec![
        "BananaTray Debug Info".to_string(),
        "=====================".to_string(),
        format!("Version:    {}", env.app_version),
        format!("OS:         {}", env.os_info),
        format!("Log Level:  {}", env.log_level),
        format!("Log Path:   {}", env.log_path),
        format!("Settings:   {}", env.settings_path),
        format!("Locale:     {}", env.locale),
        format!("Providers:  {}", env.providers_summary),
        format!("Refresh:    {}", env.refresh_interval),
    ];

    if let Some(ref size) = state.log.log_file_size {
        lines.push(format!("Log Size:   {}", size));
    }

    lines.push(String::new());
    lines.push("Provider Status:".to_string());

    for p in &state.providers {
        let quota_info = if p.quota_count > 0 {
            format!("{} quotas", p.quota_count)
        } else {
            "—".to_string()
        };
        lines.push(format!(
            "  {:<14}: {} ({})",
            p.display_name, p.status_text, quota_info
        ));
    }

    lines.join("\n")
}

fn build_console_view_state(session: &AppSession, ctx: &DebugContext) -> DebugConsoleViewState {
    // 已启用的 Provider 列表
    let available_providers: Vec<(ProviderKind, String)> = session
        .settings
        .ordered_providers()
        .into_iter()
        .filter(|kind| session.settings.is_provider_enabled(*kind))
        .filter_map(|kind| {
            session
                .provider_store
                .find(kind)
                .map(|p| (kind, p.display_name().to_string()))
        })
        .collect();

    // 从 DebugContext 注入的日志条目转换为 UI ViewState
    let log_count = ctx.captured_logs.len();
    let log_entries: Vec<CapturedLogEntry> = ctx
        .captured_logs
        .iter()
        .map(|entry| CapturedLogEntry {
            timestamp: entry.timestamp.clone(),
            level: entry.level.to_string().to_uppercase(),
            level_color: match entry.level {
                log::Level::Error => LogLevelColor::Error,
                log::Level::Warn => LogLevelColor::Warn,
                log::Level::Info => LogLevelColor::Info,
                log::Level::Debug => LogLevelColor::Debug,
                log::Level::Trace => LogLevelColor::Trace,
            },
            target: entry.target.clone(),
            message: entry.message.clone(),
        })
        .collect();

    DebugConsoleViewState {
        available_providers,
        selected_provider: session.settings_ui.debug_selected_provider,
        refresh_active: session.settings_ui.debug_refresh_active,
        log_entries,
        log_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_state::AppSession;
    use crate::models::test_helpers::make_test_provider;
    use crate::models::{AppSettings, ConnectionStatus};

    /// 构造一个不含 I/O 的测试 DebugContext
    fn test_context() -> DebugContext {
        DebugContext {
            log_level: "debug".to_string(),
            log_path: Some(PathBuf::from("/tmp/test.log")),
            log_file_size: Some(2048),
            os_info: "macOS 15.0 (aarch64)".to_string(),
            locale: "zh-CN".to_string(),
            settings_path: "/Users/test/.config/bananatray/config.toml".to_string(),
            app_version: "0.1.0".to_string(),
            captured_logs: vec![],
        }
    }

    fn make_session(settings: AppSettings) -> AppSession {
        let providers = ProviderKind::all()
            .iter()
            .map(|k| make_test_provider(*k, ConnectionStatus::Disconnected))
            .collect();
        AppSession::new(settings, providers)
    }

    fn make_session_with_status(
        settings: AppSettings,
        kind: ProviderKind,
        status: ConnectionStatus,
    ) -> AppSession {
        let providers = ProviderKind::all()
            .iter()
            .map(|k| {
                if *k == kind {
                    make_test_provider(*k, status)
                } else {
                    make_test_provider(*k, ConnectionStatus::Disconnected)
                }
            })
            .collect();
        AppSession::new(settings, providers)
    }

    // ── LogViewState 测试 ───────────────────────────────

    #[test]
    fn log_view_state_with_context() {
        let ctx = test_context();
        let vs = build_log_view_state(&ctx);
        assert_eq!(vs.current_level, "debug");
        assert_eq!(vs.log_path.as_deref(), Some("/tmp/test.log"));
        assert_eq!(vs.log_file_size.as_deref(), Some("2.0 KB"));
    }

    #[test]
    fn log_view_state_no_path() {
        let ctx = DebugContext {
            log_path: None,
            log_file_size: None,
            ..test_context()
        };
        let vs = build_log_view_state(&ctx);
        assert!(vs.log_path.is_none());
        assert!(vs.log_file_size.is_none());
    }

    #[test]
    fn log_view_state_path_but_no_size() {
        let ctx = DebugContext {
            log_path: Some(PathBuf::from("/nonexistent.log")),
            log_file_size: None, // 文件不存在
            ..test_context()
        };
        let vs = build_log_view_state(&ctx);
        assert!(vs.log_path.is_some());
        assert!(vs.log_file_size.is_none());
    }

    // ── ProviderDiagnosticItem 测试 ─────────────────────

    #[test]
    fn provider_diagnostics_all_disabled() {
        let settings = AppSettings::default();
        let session = make_session(settings);
        let items = build_provider_diagnostics(&session);

        assert!(!items.is_empty());
        for item in &items {
            assert_eq!(item.status_dot, ProviderDiagnosticStatus::Disabled);
            assert_eq!(item.status_text, "Disabled");
            assert_eq!(item.quota_count, 0);
            assert!(item.error_message.is_none());
        }
    }

    #[test]
    fn provider_diagnostics_enabled_disconnected() {
        let mut settings = AppSettings::default();
        settings.set_provider_enabled(ProviderKind::Claude, true);
        let session = make_session(settings);
        let items = build_provider_diagnostics(&session);

        let claude = items
            .iter()
            .find(|i| i.kind == ProviderKind::Claude)
            .unwrap();
        assert_eq!(claude.status_dot, ProviderDiagnosticStatus::Disconnected);
        assert!(claude.status_text.starts_with("Disconnected"));
    }

    #[test]
    fn provider_diagnostics_connected() {
        let mut settings = AppSettings::default();
        settings.set_provider_enabled(ProviderKind::Claude, true);
        let session =
            make_session_with_status(settings, ProviderKind::Claude, ConnectionStatus::Connected);
        let items = build_provider_diagnostics(&session);

        let claude = items
            .iter()
            .find(|i| i.kind == ProviderKind::Claude)
            .unwrap();
        assert_eq!(claude.status_dot, ProviderDiagnosticStatus::Connected);
        assert!(claude.status_text.starts_with("Connected"));
    }

    #[test]
    fn provider_diagnostics_refreshing() {
        let mut settings = AppSettings::default();
        settings.set_provider_enabled(ProviderKind::Cursor, true);
        let session =
            make_session_with_status(settings, ProviderKind::Cursor, ConnectionStatus::Refreshing);
        let items = build_provider_diagnostics(&session);

        let cursor = items
            .iter()
            .find(|i| i.kind == ProviderKind::Cursor)
            .unwrap();
        assert_eq!(cursor.status_dot, ProviderDiagnosticStatus::Refreshing);
        assert_eq!(cursor.status_text, "Refreshing…");
    }

    #[test]
    fn provider_diagnostics_error() {
        let mut settings = AppSettings::default();
        settings.set_provider_enabled(ProviderKind::Gemini, true);
        let mut session =
            make_session_with_status(settings, ProviderKind::Gemini, ConnectionStatus::Error);
        // 设置错误消息
        if let Some(p) = session.provider_store.find_mut(ProviderKind::Gemini) {
            p.error_message = Some("auth expired".to_string());
        }
        let items = build_provider_diagnostics(&session);

        let gemini = items
            .iter()
            .find(|i| i.kind == ProviderKind::Gemini)
            .unwrap();
        assert_eq!(gemini.status_dot, ProviderDiagnosticStatus::Error);
        assert_eq!(gemini.status_text, "Error · auth expired");
        assert_eq!(gemini.error_message.as_deref(), Some("auth expired"));
    }

    #[test]
    fn provider_diagnostics_error_without_message() {
        let mut settings = AppSettings::default();
        settings.set_provider_enabled(ProviderKind::Gemini, true);
        let session =
            make_session_with_status(settings, ProviderKind::Gemini, ConnectionStatus::Error);
        let items = build_provider_diagnostics(&session);

        let gemini = items
            .iter()
            .find(|i| i.kind == ProviderKind::Gemini)
            .unwrap();
        assert_eq!(gemini.status_text, "Error · unknown error");
    }

    #[test]
    fn provider_diagnostics_disconnected_with_error() {
        let mut settings = AppSettings::default();
        settings.set_provider_enabled(ProviderKind::Claude, true);
        let mut session = make_session(settings);
        if let Some(p) = session.provider_store.find_mut(ProviderKind::Claude) {
            p.error_message = Some("connection reset".to_string());
        }
        let items = build_provider_diagnostics(&session);

        let claude = items
            .iter()
            .find(|i| i.kind == ProviderKind::Claude)
            .unwrap();
        assert_eq!(claude.status_text, "Disconnected · connection reset");
    }

    // ── EnvironmentViewState 测试 ───────────────────────

    #[test]
    fn environment_populated_from_context() {
        let settings = AppSettings::default();
        let session = make_session(settings);
        let ctx = test_context();
        let env = build_environment_view_state(&session, &ctx);

        assert_eq!(env.app_version, "0.1.0");
        assert_eq!(env.os_info, "macOS 15.0 (aarch64)");
        assert_eq!(env.log_level, "debug");
        assert_eq!(env.locale, "zh-CN");
        assert_eq!(env.log_path, "/tmp/test.log");
    }

    #[test]
    fn environment_log_path_fallback() {
        let settings = AppSettings::default();
        let session = make_session(settings);
        let ctx = DebugContext {
            log_path: None,
            ..test_context()
        };
        let env = build_environment_view_state(&session, &ctx);
        assert_eq!(env.log_path, "—");
    }

    #[test]
    fn environment_refresh_manual() {
        let mut settings = AppSettings::default();
        settings.refresh_interval_mins = 0;
        let session = make_session(settings);
        let env = build_environment_view_state(&session, &test_context());
        assert_eq!(env.refresh_interval, "Manual");
    }

    #[test]
    fn environment_refresh_interval() {
        let mut settings = AppSettings::default();
        settings.refresh_interval_mins = 5;
        let session = make_session(settings);
        let env = build_environment_view_state(&session, &test_context());
        assert_eq!(env.refresh_interval, "5 min");
    }

    // ── build_debug_info_text 测试 ──────────────────────

    #[test]
    fn debug_info_text_structure() {
        let settings = AppSettings::default();
        let session = make_session(settings);
        let vs = debug_tab_view_state(&session, &test_context());
        let text = build_debug_info_text(&vs);

        assert!(text.contains("BananaTray Debug Info"));
        assert!(text.contains("Provider Status:"));
        assert!(text.contains("Version:    0.1.0"));
        assert!(text.contains("OS:         macOS 15.0 (aarch64)"));
        assert!(text.contains("Log Size:   2.0 KB"));
    }

    #[test]
    fn debug_info_text_omits_log_size_when_absent() {
        let settings = AppSettings::default();
        let session = make_session(settings);
        let ctx = DebugContext {
            log_file_size: None,
            ..test_context()
        };
        let vs = debug_tab_view_state(&session, &ctx);
        let text = build_debug_info_text(&vs);

        assert!(!text.contains("Log Size:"));
    }
}
