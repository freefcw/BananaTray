use crate::providers::common::http_client;
use anyhow::Result;

pub(super) const USAGE_URL: &str = "https://opencode.ai/zen/go/v1/usage";

fn usage_headers(api_key: &str) -> Vec<String> {
    vec![
        format!("Authorization: Bearer {api_key}"),
        "Accept: application/json".to_string(),
        "User-Agent: BananaTray".to_string(),
    ]
}

pub(super) fn fetch_usage(api_key: &str) -> Result<String> {
    let headers = usage_headers(api_key);
    let header_refs: Vec<_> = headers.iter().map(String::as_str).collect();
    http_client::get(USAGE_URL, &header_refs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_request_uses_bearer_auth() {
        let headers = usage_headers("sk-test");
        assert_eq!(USAGE_URL, "https://opencode.ai/zen/go/v1/usage");
        assert_eq!(headers[0], "Authorization: Bearer sk-test");
        assert!(headers.iter().any(|h| h == "Accept: application/json"));
    }
}
