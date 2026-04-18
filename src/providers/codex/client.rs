use crate::providers::common::http_client;
use anyhow::Result;

const USAGE_API_URL: &str = "https://chatgpt.com/backend-api/wham/usage";

/// 构造 usage API 请求头。
///
/// 当 `account_id` 存在时注入 `ChatGPT-Account-Id` 请求头，
/// 对齐 CodexBar `CodexOAuthUsageFetcher.fetchUsage` 的行为，
/// 确保多账号机器上 OpenAI 后端返回的是当前账号的数据。
fn usage_api_headers(access_token: &str, account_id: Option<&str>) -> Vec<String> {
    let mut headers = vec![
        format!("Authorization: Bearer {}", access_token),
        "Accept: application/json".to_string(),
        "User-Agent: OpenUsage".to_string(),
    ];
    if let Some(id) = account_id {
        headers.push(format!("ChatGPT-Account-Id: {}", id));
    }
    headers
}

pub(super) fn call_usage_api(access_token: &str, account_id: Option<&str>) -> Result<String> {
    let headers = usage_api_headers(access_token, account_id);
    let header_refs: Vec<_> = headers.iter().map(String::as_str).collect();
    http_client::get_with_headers(USAGE_API_URL, &header_refs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_api_request_uses_expected_url_and_headers() {
        let headers = usage_api_headers("token-123", None);

        assert_eq!(USAGE_API_URL, "https://chatgpt.com/backend-api/wham/usage");
        assert_eq!(headers.len(), 3);
        assert_eq!(headers[0], "Authorization: Bearer token-123");
        assert_eq!(headers[1], "Accept: application/json");
        assert_eq!(headers[2], "User-Agent: OpenUsage");
    }

    #[test]
    fn usage_api_request_injects_account_id_header_when_present() {
        let headers = usage_api_headers("token-123", Some("acct_abc"));
        assert_eq!(headers.len(), 4);
        assert_eq!(headers[3], "ChatGPT-Account-Id: acct_abc");
    }

    #[test]
    fn usage_api_request_omits_account_id_header_when_absent() {
        let headers = usage_api_headers("token-123", None);
        assert!(headers.iter().all(|h| !h.starts_with("ChatGPT-Account-Id")));
    }
}
