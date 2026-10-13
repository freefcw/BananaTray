use crate::providers::common::http_client;
use anyhow::Result;

pub(super) const GLOBAL_QUOTA_URL: &str = "https://api.z.ai/api/monitor/usage/quota/limit";
pub(super) const CHINA_QUOTA_URL: &str = "https://open.bigmodel.cn/api/monitor/usage/quota/limit";

pub(super) fn fetch_quota(url: &str, api_key: &str) -> Result<String> {
    let authorization = format!("Authorization: Bearer {api_key}");
    let headers = [
        authorization.as_str(),
        "Accept: application/json",
        "User-Agent: BananaTray",
    ];
    http_client::get(url, &headers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quota_urls_stay_on_official_gateways() {
        assert_eq!(
            GLOBAL_QUOTA_URL,
            "https://api.z.ai/api/monitor/usage/quota/limit"
        );
        assert_eq!(
            CHINA_QUOTA_URL,
            "https://open.bigmodel.cn/api/monitor/usage/quota/limit"
        );
    }
}
