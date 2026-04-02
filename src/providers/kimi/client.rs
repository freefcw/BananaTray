use crate::utils::http_client;
use anyhow::Result;

pub(super) fn fetch_usage(token: &str) -> Result<String> {
    let auth_header = format!("Authorization: Bearer {}", token);
    let cookie_header = format!("Cookie: kimi-auth={}", token);

    http_client::post_json(
        "https://www.kimi.com/apiv2/kimi.gateway.billing.v1.BillingService/GetUsages",
        &[
            &auth_header,
            &cookie_header,
            "Origin: https://www.kimi.com",
            "Referer: https://www.kimi.com/code/console",
            "Accept: */*",
            "User-Agent: Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36",
            "connect-protocol-version: 1",
            "x-language: en-US",
            "x-msh-platform: web",
        ],
        r#"{"scope":["FEATURE_CODING"]}"#,
    )
}
