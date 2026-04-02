use crate::utils::http_client;
use anyhow::Result;

pub(super) fn fetch_usage_summary(cookie: &str) -> Result<String> {
    let cookie_header = format!("Cookie: {}", cookie);
    http_client::get(
        "https://cursor.com/api/usage-summary",
        &[&cookie_header, "Content-Type: application/json"],
    )
}
