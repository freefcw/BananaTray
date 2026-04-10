use super::parser::parse_quota_response;
use crate::models::QuotaInfo;
use crate::providers::common::http_client;
use anyhow::Result;

pub(super) fn fetch_quota_via_api(access_token: &str) -> Result<Vec<QuotaInfo>> {
    let url = "https://cloudcode-pa.googleapis.com/v1internal:retrieveUserQuota";
    let auth_header = format!("Authorization: Bearer {}", access_token);

    let response_str =
        http_client::post_json(url, &[&auth_header, "Accept: application/json"], "{}")?;
    parse_quota_response(&response_str)
}
