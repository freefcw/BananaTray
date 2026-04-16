use crate::providers::common::http_client;
use anyhow::Result;

const USAGE_SUMMARY_URL: &str = "https://cursor.com/api/usage-summary";

fn usage_summary_headers(cookie: &str) -> [String; 2] {
    [
        format!("Cookie: {}", cookie),
        "Content-Type: application/json".to_string(),
    ]
}

pub(super) fn fetch_usage_summary(cookie: &str) -> Result<String> {
    let headers = usage_summary_headers(cookie);
    let header_refs: Vec<_> = headers.iter().map(String::as_str).collect();
    http_client::get(USAGE_SUMMARY_URL, &header_refs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_summary_request_uses_expected_url_and_headers() {
        let headers = usage_summary_headers("cookie=value");

        assert_eq!(USAGE_SUMMARY_URL, "https://cursor.com/api/usage-summary");
        assert_eq!(headers[0], "Cookie: cookie=value");
        assert_eq!(headers[1], "Content-Type: application/json");
    }
}
