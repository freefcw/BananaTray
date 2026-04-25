//! Issue Report 纯函数 selector
//!
//! 收集环境信息 + Provider 状态，生成 Markdown 格式的 Issue 报告，
//! 并构造 GitHub Issue URL（title + body 预填）。
//!
//! 设计：所有 I/O 由 runtime/ui infrastructure 收集进 `IssueReportContext`，
//! selector 本身是纯函数，可单元测试。

use super::super::state::AppSession;
use crate::utils::text_utils::url_encode;

/// GitHub issue 创建页 URL 前缀
const ISSUE_URL_BASE: &str = concat!(env!("CARGO_PKG_REPOSITORY"), "/issues/new");

// ============================================================================
// 运行时上下文（在调用 selector 之前由 runtime/ui infrastructure 构造）
// ============================================================================

/// Issue Report 所需的运行时信息
#[derive(Debug, Clone)]
pub struct IssueReportContext {
    pub app_version: String,
    pub git_hash: String,
    pub os_info: String,
    pub locale: String,
    pub log_level: String,
    /// 最近 30 分钟的 WARN/ERROR 日志
    pub recent_errors: String,
}

// ============================================================================
// 报告数据结构
// ============================================================================

/// 生成的 Issue 报告
#[derive(Debug, Clone)]
pub struct IssueReport {
    pub title: String,
    pub body: String,
}

// ============================================================================
// Selector 纯函数
// ============================================================================

/// 构建 Issue 报告（纯函数）
pub fn build_issue_report(session: &AppSession, ctx: &IssueReportContext) -> IssueReport {
    let title = format!("[Bug Report] BananaTray v{}", ctx.app_version);

    // ── 已启用的 Provider 列表 ──
    let enabled_names: Vec<&str> = session
        .provider_store
        .providers
        .iter()
        .filter(|p| session.settings.provider.is_enabled(&p.provider_id))
        .map(|p| p.display_name())
        .collect();

    let mut body = String::with_capacity(2048);

    // ── 环境信息（简洁 key: value 格式）──
    body.push_str("## Environment\n\n");
    body.push_str(&format!(
        "- Version: {} ({})\n",
        ctx.app_version, ctx.git_hash
    ));
    body.push_str(&format!("- OS: {}\n", ctx.os_info));
    body.push_str(&format!("- Locale: {}\n", ctx.locale));
    body.push_str(&format!("- Log Level: {}\n", ctx.log_level));
    body.push_str(&format!(
        "- Enabled Providers: [{}]\n",
        enabled_names.join(", ")
    ));

    // ── 问题描述占位 ──
    body.push_str("\n## Description\n\n");
    body.push_str("<!-- Please describe the issue here -->\n\n");

    // ── 最近 30 分钟内的 WARN/ERROR 日志 ──
    if !ctx.recent_errors.is_empty() {
        body.push_str("## Recent Errors\n\n");
        body.push_str("```\n");
        body.push_str(&ctx.recent_errors);
        body.push_str("\n```\n");
    }

    IssueReport { title, body }
}

/// 构建 GitHub issue/new URL（title + body 预填）
pub fn build_issue_url(report: &IssueReport) -> String {
    let title_encoded = url_encode(&report.title);
    let body_encoded = url_encode(&report.body);
    format!(
        "{}?title={}&body={}",
        ISSUE_URL_BASE, title_encoded, body_encoded
    )
}

#[cfg(test)]
#[path = "issue_report_tests.rs"]
mod tests;
