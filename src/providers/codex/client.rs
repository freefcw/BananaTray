use crate::providers::common::http_client;
use anyhow::Result;

pub(super) fn call_usage_api(access_token: &str) -> Result<String> {
    let auth_header = format!("Authorization: Bearer {}", access_token);
    http_client::get_with_headers(
        "https://chatgpt.com/backend-api/wham/usage",
        &[
            &auth_header,
            "Accept: application/json",
            "User-Agent: OpenUsage",
        ],
    )
}
