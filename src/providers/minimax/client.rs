use crate::providers::common::http_client;
use anyhow::Result;

pub(super) fn fetch_remains(url: &str, api_key: &str) -> Result<String> {
    let auth_header = format!("Authorization: Bearer {}", api_key);
    http_client::get(url, &[&auth_header])
}
