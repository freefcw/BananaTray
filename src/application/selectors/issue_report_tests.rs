use super::*;
use crate::application::state::AppSession;
use crate::models::AppSettings;

/// 构造测试用的 IssueReportContext（无 I/O）
fn make_ctx() -> IssueReportContext {
    IssueReportContext {
        app_version: "1.2.3".to_string(),
        git_hash: "abc1234".to_string(),
        os_info: "macOS 15.4 (aarch64)".to_string(),
        locale: "en".to_string(),
        log_level: "info".to_string(),
        recent_errors: String::new(),
    }
}

fn make_session() -> AppSession {
    let settings = AppSettings::default();
    let providers = vec![];
    AppSession::new(settings, providers)
}

#[test]
fn report_title_contains_version() {
    let session = make_session();
    let ctx = make_ctx();
    let report = build_issue_report(&session, &ctx);
    assert!(report.title.contains("1.2.3"));
    assert!(report.title.contains("[Bug Report]"));
}

#[test]
fn report_body_contains_environment_info() {
    let session = make_session();
    let ctx = make_ctx();
    let report = build_issue_report(&session, &ctx);

    assert!(report.body.contains("## Environment"));
    assert!(report.body.contains("1.2.3 (abc1234)"));
    assert!(report.body.contains("macOS 15.4 (aarch64)"));
    assert!(report.body.contains("Locale: en"));
    assert!(report.body.contains("Log Level: info"));
}

#[test]
fn report_body_uses_simple_format_not_table() {
    let session = make_session();
    let ctx = make_ctx();
    let report = build_issue_report(&session, &ctx);
    // 不使用 Markdown 表格
    assert!(!report.body.contains("| Key | Value |"));
    assert!(!report.body.contains("|-----|"));
    // 使用简洁 list 格式
    assert!(report.body.contains("- Version:"));
    assert!(report.body.contains("- OS:"));
}

#[test]
fn report_body_does_not_contain_settings_path() {
    let session = make_session();
    let ctx = make_ctx();
    let report = build_issue_report(&session, &ctx);
    assert!(!report.body.contains("Settings"));
}

#[test]
fn report_body_contains_description_placeholder() {
    let session = make_session();
    let ctx = make_ctx();
    let report = build_issue_report(&session, &ctx);
    assert!(report.body.contains("## Description"));
    assert!(report
        .body
        .contains("<!-- Please describe the issue here -->"));
}

#[test]
fn report_body_shows_enabled_providers_list() {
    use crate::models::test_helpers::make_test_provider;
    use crate::models::{ConnectionStatus, ProviderId, ProviderKind};

    let mut settings = AppSettings::default();
    let providers = vec![
        make_test_provider(ProviderKind::Claude, ConnectionStatus::Connected),
        make_test_provider(ProviderKind::Copilot, ConnectionStatus::Disconnected),
    ];
    // 启用 Claude，禁用 Copilot
    settings
        .provider
        .set_enabled(&ProviderId::BuiltIn(ProviderKind::Claude), true);
    settings
        .provider
        .set_enabled(&ProviderId::BuiltIn(ProviderKind::Copilot), false);
    let session = AppSession::new(settings, providers);
    let ctx = make_ctx();

    let report = build_issue_report(&session, &ctx);
    assert!(report.body.contains("Enabled Providers:"));
    assert!(report.body.contains("Claude"));
    // Copilot 已禁用，不应在列表中
    assert!(!report.body.contains("Copilot"));
}

#[test]
fn report_body_includes_recent_errors() {
    let session = make_session();
    let mut ctx = make_ctx();
    ctx.recent_errors = "2026-04-12 09:30:00.123 [ERROR] providers   fetch failed".to_string();

    let report = build_issue_report(&session, &ctx);
    assert!(report.body.contains("## Recent Errors"));
    assert!(report.body.contains("fetch failed"));
}

#[test]
fn report_body_omits_errors_section_when_empty() {
    let session = make_session();
    let ctx = make_ctx();
    let report = build_issue_report(&session, &ctx);
    // 无错误日志时不显示 Recent Errors 区域
    assert!(!report.body.contains("## Recent Errors"));
}

#[test]
fn build_issue_url_contains_title_and_body() {
    let report = IssueReport {
        title: "[Bug Report] v1.0".to_string(),
        body: "hello world".to_string(),
    };
    let url = build_issue_url(&report);
    assert!(url.starts_with(concat!(env!("CARGO_PKG_REPOSITORY"), "/issues/new?")));
    assert!(url.contains("title="));
    assert!(url.contains("body="));
    assert!(url.contains("%20"));
}

#[test]
fn report_url_is_short_without_errors() {
    let session = make_session();
    let ctx = make_ctx();
    let report = build_issue_report(&session, &ctx);
    let url = build_issue_url(&report);
    assert!(url.len() < 2000, "URL too long: {} chars", url.len());
}
