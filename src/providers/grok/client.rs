use crate::providers::common::http_client;
use anyhow::Result;

pub(super) const BILLING_URL: &str = "https://cli-chat-proxy.grok.com/v1/billing?format=credits";

fn billing_headers(access_token: &str) -> Vec<String> {
    vec![
        format!("Authorization: Bearer {access_token}"),
        "x-xai-token-auth: xai-grok-cli".to_string(),
        "Accept: application/json".to_string(),
        "User-Agent: BananaTray".to_string(),
    ]
}

pub(super) fn fetch_billing(access_token: &str) -> Result<String> {
    let headers = billing_headers(access_token);
    let header_refs: Vec<_> = headers.iter().map(String::as_str).collect();
    // `get_with_headers` 返回状态行 + 响应头 + 正文，给 Codex 这种 raw HTTP 解析器用。
    // 这里只要 JSON body，必须用 `get`。
    http_client::get(BILLING_URL, &header_refs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn billing_request_headers_use_bearer_token() {
        let headers = billing_headers("token-123");
        assert_eq!(headers[0], "Authorization: Bearer token-123");
        assert_eq!(headers[1], "x-xai-token-auth: xai-grok-cli");
        assert_eq!(headers[2], "Accept: application/json");
        assert_eq!(headers[3], "User-Agent: BananaTray");
    }
}
