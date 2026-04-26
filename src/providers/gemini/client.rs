use super::parser::parse_quota_response;
use crate::models::QuotaInfo;
use crate::providers::common::http_client;
use anyhow::Result;

const QUOTA_API_URL: &str = "https://cloudcode-pa.googleapis.com/v1internal:retrieveUserQuota";
const QUOTA_API_BODY: &str = "{}";

fn quota_api_headers(access_token: &str) -> [String; 2] {
    [
        format!("Authorization: Bearer {}", access_token),
        "Accept: application/json".to_string(),
    ]
}

pub(super) fn fetch_quota_via_api(access_token: &str) -> Result<Vec<QuotaInfo>> {
    let headers = quota_api_headers(access_token);
    let header_refs: Vec<_> = headers.iter().map(String::as_str).collect();

    let response_str = http_client::post_json(QUOTA_API_URL, &header_refs, QUOTA_API_BODY)?;
    parse_quota_response(&response_str).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quota_api_request_uses_expected_url_headers_and_body() {
        let headers = quota_api_headers("ya29.token");

        assert_eq!(
            QUOTA_API_URL,
            "https://cloudcode-pa.googleapis.com/v1internal:retrieveUserQuota"
        );
        assert_eq!(QUOTA_API_BODY, "{}");
        assert_eq!(headers[0], "Authorization: Bearer ya29.token");
        assert_eq!(headers[1], "Accept: application/json");
    }
}
