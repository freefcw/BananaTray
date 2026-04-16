use crate::providers::common::http_client;
use anyhow::Result;

fn auth_header(api_key: &str) -> String {
    format!("Authorization: Bearer {}", api_key)
}

pub(super) fn fetch_remains(url: &str, api_key: &str) -> Result<String> {
    let auth_header = auth_header(api_key);
    http_client::get(url, &[auth_header.as_str()])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimax_request_uses_bearer_header() {
        assert_eq!(auth_header("mm-key"), "Authorization: Bearer mm-key");
    }
}
