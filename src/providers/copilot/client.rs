use crate::providers::common::http_client;
use anyhow::Result;

const COPILOT_USER_URL: &str = "https://api.github.com/copilot_internal/user";
const GITHUB_USER_URL: &str = "https://api.github.com/user";

fn github_api_headers(token: &str) -> [String; 2] {
    [
        format!("Authorization: Bearer {}", token),
        "Accept: application/json".to_string(),
    ]
}

pub(super) fn fetch_user_info(token: &str) -> Result<(String, String)> {
    let headers = github_api_headers(token);
    let header_refs: Vec<_> = headers.iter().map(String::as_str).collect();
    http_client::get_with_status(COPILOT_USER_URL, &header_refs)
}

/// 获取 GitHub 用户基本信息（login / email）
pub(super) fn fetch_github_user(token: &str) -> Result<(String, String)> {
    let headers = github_api_headers(token);
    let header_refs: Vec<_> = headers.iter().map(String::as_str).collect();
    http_client::get_with_status(GITHUB_USER_URL, &header_refs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copilot_requests_use_expected_urls_and_headers() {
        let headers = github_api_headers("ghp_test");

        assert_eq!(
            COPILOT_USER_URL,
            "https://api.github.com/copilot_internal/user"
        );
        assert_eq!(GITHUB_USER_URL, "https://api.github.com/user");
        assert_eq!(headers[0], "Authorization: Bearer ghp_test");
        assert_eq!(headers[1], "Accept: application/json");
    }
}
