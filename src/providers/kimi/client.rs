use crate::providers::common::http_client;
use anyhow::Result;

const USAGE_URL: &str =
    "https://www.kimi.com/apiv2/kimi.gateway.billing.v1.BillingService/GetUsages";
const USAGE_BODY: &str = r#"{"scope":["FEATURE_CODING"]}"#;

fn usage_headers(token: &str) -> Vec<String> {
    vec![
        format!("Authorization: Bearer {}", token),
        format!("Cookie: kimi-auth={}", token),
        "Origin: https://www.kimi.com".to_string(),
        "Referer: https://www.kimi.com/code/console".to_string(),
        "Accept: */*".to_string(),
        "User-Agent: Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36"
            .to_string(),
        "connect-protocol-version: 1".to_string(),
        "x-language: en-US".to_string(),
        "x-msh-platform: web".to_string(),
    ]
}

pub(super) fn fetch_usage(token: &str) -> Result<String> {
    let headers = usage_headers(token);
    let header_refs: Vec<_> = headers.iter().map(String::as_str).collect();
    http_client::post_json(USAGE_URL, &header_refs, USAGE_BODY)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kimi_usage_request_uses_expected_url_headers_and_body() {
        let headers = usage_headers("kimi-token");

        assert_eq!(
            USAGE_URL,
            "https://www.kimi.com/apiv2/kimi.gateway.billing.v1.BillingService/GetUsages"
        );
        assert_eq!(USAGE_BODY, r#"{"scope":["FEATURE_CODING"]}"#);
        assert_eq!(headers[0], "Authorization: Bearer kimi-token");
        assert_eq!(headers[1], "Cookie: kimi-auth=kimi-token");
        assert!(headers.iter().any(|h| h == "Origin: https://www.kimi.com"));
        assert!(headers
            .iter()
            .any(|h| h == "Referer: https://www.kimi.com/code/console"));
    }
}
