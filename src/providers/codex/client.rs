use crate::providers::common::http_client;
use anyhow::Result;

const USAGE_API_URL: &str = "https://chatgpt.com/backend-api/wham/usage";

fn usage_api_headers(access_token: &str) -> [String; 3] {
    [
        format!("Authorization: Bearer {}", access_token),
        "Accept: application/json".to_string(),
        "User-Agent: OpenUsage".to_string(),
    ]
}

pub(super) fn call_usage_api(access_token: &str) -> Result<String> {
    let headers = usage_api_headers(access_token);
    let header_refs: Vec<_> = headers.iter().map(String::as_str).collect();
    http_client::get_with_headers(USAGE_API_URL, &header_refs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_api_request_uses_expected_url_and_headers() {
        let headers = usage_api_headers("token-123");

        assert_eq!(USAGE_API_URL, "https://chatgpt.com/backend-api/wham/usage");
        assert_eq!(headers[0], "Authorization: Bearer token-123");
        assert_eq!(headers[1], "Accept: application/json");
        assert_eq!(headers[2], "User-Agent: OpenUsage");
    }
}
