//! Shared HTTP client utilities for providers.
//!
//! Uses `ureq` for type-safe HTTP requests instead of shelling out to `curl`.

use anyhow::{bail, Context, Result};
use log::{debug, warn};
use std::sync::LazyLock;
use std::time::Duration;
use ureq::Agent;

const HTTP_TIMEOUT: Duration = Duration::from_secs(20);

static AGENT: LazyLock<Agent> = LazyLock::new(|| {
    Agent::new_with_config(
        ureq::config::Config::builder()
            .http_status_as_error(false)
            .timeout_global(Some(HTTP_TIMEOUT))
            .build(),
    )
});

/// Parse a raw header string like `"Authorization: Bearer xxx"` into (name, value).
///
/// Uses `split_once(':')` so that colons in the *value* part are preserved
/// (e.g. `"Authorization: Bearer abc:def"` → `("Authorization", "Bearer abc:def")`).
fn parse_header(h: &str) -> Option<(&str, &str)> {
    let (name, value) = h.split_once(':')?;
    Some((name.trim(), value.trim()))
}

macro_rules! set_headers {
    ($req:expr, $headers:expr) => {{
        let mut req = $req;
        for h in $headers {
            if let Some((name, value)) = parse_header(h) {
                req = req.header(name, value);
            }
        }
        req
    }};
}

fn map_http_error(err: ureq::Error) -> anyhow::Error {
    match err {
        ureq::Error::Timeout(_) => crate::providers::ProviderError::Timeout.into(),
        other => anyhow::anyhow!(other),
    }
}

/// Perform an HTTP GET and return the response body as a String.
///
/// `headers` is a list of header strings like `"Authorization: Bearer xxx"`.
#[allow(dead_code)]
pub fn get(url: &str, headers: &[&str]) -> Result<String> {
    debug!(target: "http", "GET {}", url);

    let response = set_headers!(AGENT.get(url), headers)
        .call()
        .map_err(map_http_error)?;

    let status = response.status().as_u16();
    debug!(target: "http", "GET {} -> {}", url, status);

    if status >= 400 {
        let body = response
            .into_body()
            .read_to_string()
            .unwrap_or_else(|_| "<unable to read body>".to_string());
        warn!(target: "http", "GET {} failed with status {}, body: {}", url, status, body);
        bail!("HTTP GET {url} returned status {status}: {body}");
    }

    response
        .into_body()
        .read_to_string()
        .map_err(map_http_error)
        .with_context(|| format!("Failed to read response body from {url}"))
}

/// Perform an HTTP GET and return the full raw output (headers + body).
///
/// The response is formatted as `"HTTP/1.1 <status>\r\n<headers>\r\n\r\n<body>"`
/// to maintain compatibility with callers that parse raw HTTP responses (e.g. Codex).
pub fn get_with_headers(url: &str, headers: &[&str]) -> Result<String> {
    debug!(target: "http", "GET {} (with headers)", url);

    let response = set_headers!(AGENT.get(url), headers)
        .call()
        .map_err(map_http_error)?;

    let status = response.status().as_u16();

    let mut raw = format!("HTTP/1.1 {status}\r\n");
    for name in response.headers().keys() {
        if let Some(value) = response.headers().get(name) {
            raw.push_str(&format!(
                "{}: {}\r\n",
                name.as_str(),
                value.to_str().unwrap_or("")
            ));
        }
    }
    raw.push_str("\r\n");

    let body = response
        .into_body()
        .read_to_string()
        .map_err(map_http_error)
        .with_context(|| format!("Failed to read response body from {url}"))?;
    raw.push_str(&body);

    Ok(raw)
}

/// Perform an HTTP GET and return `(body, http_status_code)`.
pub fn get_with_status(url: &str, headers: &[&str]) -> Result<(String, String)> {
    debug!(target: "http", "GET {}", url);

    let response = set_headers!(AGENT.get(url), headers)
        .call()
        .map_err(map_http_error)?;

    let status = response.status().as_u16().to_string();
    debug!(target: "http", "GET {} -> {}", url, status);

    let body = response
        .into_body()
        .read_to_string()
        .map_err(map_http_error)
        .with_context(|| format!("Failed to read response body from {url}"))?;

    Ok((body, status))
}

/// Perform an HTTP POST with a JSON body (Content-Type: application/json).
pub fn post_json(url: &str, headers: &[&str], body: &str) -> Result<String> {
    debug!(target: "http", "POST {} ({} bytes)", url, body.len());

    let response = set_headers!(
        AGENT.post(url).header("Content-Type", "application/json"),
        headers
    )
    .send(body.as_bytes())
    .map_err(map_http_error)?;

    let status = response.status().as_u16();
    debug!(target: "http", "POST {} -> {}", url, status);

    if status >= 400 {
        let body = response
            .into_body()
            .read_to_string()
            .unwrap_or_else(|_| "<unable to read body>".to_string());
        warn!(target: "http", "POST {} failed with status {}, body: {}", url, status, body);
        bail!("HTTP POST {url} returned status {status}: {body}");
    }

    response
        .into_body()
        .read_to_string()
        .map_err(map_http_error)
        .with_context(|| format!("Failed to read response body from POST {url}"))
}

/// Perform an HTTP POST with a form-urlencoded body.
pub fn post_form(url: &str, headers: &[&str], body: &str) -> Result<String> {
    debug!(target: "http", "POST {} (form, {} bytes)", url, body.len());

    let response = set_headers!(
        AGENT
            .post(url)
            .header("Content-Type", "application/x-www-form-urlencoded"),
        headers
    )
    .send(body.as_bytes())
    .map_err(map_http_error)?;

    let status = response.status().as_u16();
    debug!(target: "http", "POST {} -> {}", url, status);

    if status >= 400 {
        let body = response
            .into_body()
            .read_to_string()
            .unwrap_or_else(|_| "<unable to read body>".to_string());
        warn!(target: "http", "POST {} (form) failed with status {}, body: {}", url, status, body);
        bail!("HTTP POST {url} returned status {status}: {body}");
    }

    response
        .into_body()
        .read_to_string()
        .map_err(map_http_error)
        .with_context(|| format!("Failed to read response body from POST {url}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_header_basic() {
        let (name, value) = parse_header("Authorization: Bearer token123").unwrap();
        assert_eq!(name, "Authorization");
        assert_eq!(value, "Bearer token123");
    }

    #[test]
    fn test_parse_header_value_with_colons() {
        let (name, value) = parse_header("Authorization: Bearer abc:def:ghi").unwrap();
        assert_eq!(name, "Authorization");
        assert_eq!(value, "Bearer abc:def:ghi");
    }

    #[test]
    fn test_parse_header_trims_whitespace() {
        let (name, value) = parse_header("  Accept  :   application/json  ").unwrap();
        assert_eq!(name, "Accept");
        assert_eq!(value, "application/json");
    }

    #[test]
    fn test_parse_header_no_colon() {
        assert!(parse_header("no-colon-here").is_none());
    }

    #[test]
    fn test_parse_header_empty() {
        assert!(parse_header("").is_none());
    }
}
