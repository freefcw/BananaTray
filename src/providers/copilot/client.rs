use crate::utils::http_client;
use anyhow::Result;

pub(super) fn fetch_user_info(token: &str) -> Result<(String, String)> {
    let auth_header = format!("Authorization: Bearer {}", token);
    http_client::get_with_status(
        "https://api.github.com/copilot_internal/user",
        &[&auth_header, "Accept: application/json"],
    )
}
