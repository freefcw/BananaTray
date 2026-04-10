use super::*;
use crate::application::AppSession;
use crate::models::test_helpers::make_test_provider;
use crate::models::{AppSettings, ConnectionStatus, ProviderKind};

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
    settings
        .provider
        .set_provider_enabled(ProviderKind::Claude, true);
    let session = make_session(settings);
    let items = build_provider_diagnostics(&session);

    let claude = items
        .iter()
        .find(|i| i.id == ProviderId::BuiltIn(ProviderKind::Claude))
        .unwrap();
    assert_eq!(claude.status_dot, ProviderDiagnosticStatus::Disconnected);
    assert!(claude.status_text.starts_with("Disconnected"));
}

#[test]
fn provider_diagnostics_connected() {
    let mut settings = AppSettings::default();
    settings
        .provider
        .set_provider_enabled(ProviderKind::Claude, true);
    let session =
        make_session_with_status(settings, ProviderKind::Claude, ConnectionStatus::Connected);
    let items = build_provider_diagnostics(&session);

    let claude = items
        .iter()
        .find(|i| i.id == ProviderId::BuiltIn(ProviderKind::Claude))
        .unwrap();
    assert_eq!(claude.status_dot, ProviderDiagnosticStatus::Connected);
    assert!(claude.status_text.starts_with("Connected"));
}

#[test]
fn provider_diagnostics_refreshing() {
    let mut settings = AppSettings::default();
    settings
        .provider
        .set_provider_enabled(ProviderKind::Cursor, true);
    let session =
        make_session_with_status(settings, ProviderKind::Cursor, ConnectionStatus::Refreshing);
    let items = build_provider_diagnostics(&session);

    let cursor = items
        .iter()
        .find(|i| i.id == ProviderId::BuiltIn(ProviderKind::Cursor))
        .unwrap();
    assert_eq!(cursor.status_dot, ProviderDiagnosticStatus::Refreshing);
    assert_eq!(cursor.status_text, "Refreshing…");
}

#[test]
fn provider_diagnostics_error() {
    let mut settings = AppSettings::default();
    settings
        .provider
        .set_provider_enabled(ProviderKind::Gemini, true);
    let mut session =
        make_session_with_status(settings, ProviderKind::Gemini, ConnectionStatus::Error);
    if let Some(p) = session
        .provider_store
        .find_by_id_mut(&ProviderId::BuiltIn(ProviderKind::Gemini))
    {
        p.error_message = Some("auth expired".to_string());
    }
    let items = build_provider_diagnostics(&session);

    let gemini = items
        .iter()
        .find(|i| i.id == ProviderId::BuiltIn(ProviderKind::Gemini))
        .unwrap();
    assert_eq!(gemini.status_dot, ProviderDiagnosticStatus::Error);
    assert_eq!(gemini.status_text, "Error · auth expired");
    assert_eq!(gemini.error_message.as_deref(), Some("auth expired"));
}

#[test]
fn provider_diagnostics_error_without_message() {
    let mut settings = AppSettings::default();
    settings
        .provider
        .set_provider_enabled(ProviderKind::Gemini, true);
    let session = make_session_with_status(settings, ProviderKind::Gemini, ConnectionStatus::Error);
    let items = build_provider_diagnostics(&session);

    let gemini = items
        .iter()
        .find(|i| i.id == ProviderId::BuiltIn(ProviderKind::Gemini))
        .unwrap();
    assert_eq!(gemini.status_text, "Error · unknown error");
}

#[test]
fn provider_diagnostics_disconnected_with_error() {
    let mut settings = AppSettings::default();
    settings
        .provider
        .set_provider_enabled(ProviderKind::Claude, true);
    let mut session = make_session(settings);
    if let Some(p) = session
        .provider_store
        .find_by_id_mut(&ProviderId::BuiltIn(ProviderKind::Claude))
    {
        p.error_message = Some("connection reset".to_string());
    }
    let items = build_provider_diagnostics(&session);

    let claude = items
        .iter()
        .find(|i| i.id == ProviderId::BuiltIn(ProviderKind::Claude))
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
    let settings = AppSettings {
        system: crate::models::SystemSettings {
            refresh_interval_mins: 0,
            ..Default::default()
        },
        ..Default::default()
    };
    let session = make_session(settings);
    let env = build_environment_view_state(&session, &test_context());
    assert_eq!(env.refresh_interval, "Manual");
}

#[test]
fn environment_refresh_interval() {
    let settings = AppSettings {
        system: crate::models::SystemSettings {
            refresh_interval_mins: 5,
            ..Default::default()
        },
        ..Default::default()
    };
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
