use crate::providers::common::http_client;
use anyhow::Result;

pub(super) fn fetch_user_info(token: &str) -> Result<(String, String)> {
    let auth_header = format!("Authorization: Bearer {}", token);
    http_client::get_with_status(
        "https://api.github.com/copilot_internal/user",
        &[&auth_header, "Accept: application/json"],
    )
}

/// 获取 GitHub 用户基本信息（login / email）
pub(super) fn fetch_github_user(token: &str) -> Result<(String, String)> {
    let auth_header = format!("Authorization: Bearer {}", token);
    http_client::get_with_status(
        "https://api.github.com/user",
        &[&auth_header, "Accept: application/json"],
    )
}
