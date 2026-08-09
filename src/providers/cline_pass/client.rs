use crate::providers::common::http_client;
use anyhow::Result;

pub(super) const USAGE_URL: &str = "https://api.cline.bot/api/v1/users/me/plan/usage-limits";

pub(super) fn auth_header(token: &str) -> String {
    format!("Authorization: Bearer {token}")
}

pub(super) fn fetch_usage(url: &str, token: &str) -> Result<String> {
    let auth_header = auth_header(token);
    http_client::get(url, &[auth_header.as_str()])
}
