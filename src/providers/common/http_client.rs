//! Shared HTTP client utilities for providers.
//!
//! Uses `ureq` for type-safe HTTP requests instead of shelling out to `curl`.

use anyhow::{Context, Result};
use log::{debug, warn};
use std::fmt;
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

// ── 结构化 HTTP 错误 ──────────────────────────────────

/// HTTP 层结构化错误，provider 可通过 `downcast_ref::<HttpError>()` 精确分类。
#[derive(Debug, Clone)]
pub enum HttpError {
    /// 请求超时
    Timeout,
    /// 传输层错误（DNS / 连接 / TLS 等）
    Transport(String),
    /// 服务端返回了 HTTP 错误状态码
    HttpStatus { code: u16, body: String },
}

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Timeout => write!(f, "request timeout"),
            Self::Transport(reason) => write!(f, "transport error: {}", reason),
            Self::HttpStatus { code, body } => {
                write!(f, "HTTP status {}: {}", code, body)
            }
        }
    }
}

impl std::error::Error for HttpError {}

impl HttpError {
    /// 是否为认证类错误（401 / 403）
    pub fn is_auth_error(&self) -> bool {
        matches!(self, Self::HttpStatus { code, .. } if *code == 401 || *code == 403)
    }
}

// ── 内部工具 ──────────────────────────────────────────

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

/// 将 ureq 传输层错误映射为 HttpError
fn map_transport_error(err: ureq::Error) -> HttpError {
    match err {
        ureq::Error::Timeout(_) => HttpError::Timeout,
        other => HttpError::Transport(other.to_string()),
    }
}

/// 检查 HTTP 响应状态码，4xx/5xx 返回 HttpError::HttpStatus
fn check_status(
    status: u16,
    url: &str,
    method: &str,
    response: ureq::http::Response<ureq::Body>,
) -> Result<ureq::http::Response<ureq::Body>> {
    if status >= 400 {
        let body = response
            .into_body()
            .read_to_string()
            .unwrap_or_else(|_| "<unable to read body>".to_string());
        warn!(target: "http", "{} {} failed with status {}, body: {}", method, url, status, body);
        return Err(HttpError::HttpStatus { code: status, body }.into());
    }
    Ok(response)
}

/// Perform an HTTP GET and return the response body as a String.
///
/// `headers` is a list of header strings like `"Authorization: Bearer xxx"`.
///
/// 4xx/5xx → `HttpError::HttpStatus`，超时 → `HttpError::Timeout`
#[allow(dead_code)]
pub fn get(url: &str, headers: &[&str]) -> Result<String> {
    debug!(target: "http", "GET {}", url);

    let response = set_headers!(AGENT.get(url), headers)
        .call()
        .map_err(|e| anyhow::Error::from(map_transport_error(e)))?;

    let status = response.status().as_u16();
    debug!(target: "http", "GET {} -> {}", url, status);

    let response = check_status(status, url, "GET", response)?;

    response
        .into_body()
        .read_to_string()
        .map_err(|e| anyhow::Error::from(map_transport_error(e)))
        .with_context(|| format!("Failed to read response body from {url}"))
}

/// Perform an HTTP GET and return the full raw output (headers + body).
///
/// The response is formatted as `"HTTP/1.1 <status>\r\n<headers>\r\n\r\n<body>"`
/// to maintain compatibility with callers that parse raw HTTP responses (e.g. Codex).
///
/// 4xx/5xx → `HttpError::HttpStatus`，超时 → `HttpError::Timeout`
pub fn get_with_headers(url: &str, headers: &[&str]) -> Result<String> {
    debug!(target: "http", "GET {} (with headers)", url);

    let response = set_headers!(AGENT.get(url), headers)
        .call()
        .map_err(|e| anyhow::Error::from(map_transport_error(e)))?;

    let status = response.status().as_u16();

    let response = check_status(status, url, "GET", response)?;

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
        .map_err(|e| anyhow::Error::from(map_transport_error(e)))
        .with_context(|| format!("Failed to read response body from {url}"))?;
    raw.push_str(&body);

    Ok(raw)
}

/// Perform an HTTP POST with a JSON body (Content-Type: application/json).
///
/// 4xx/5xx → `HttpError::HttpStatus`，超时 → `HttpError::Timeout`
pub fn post_json(url: &str, headers: &[&str], body: &str) -> Result<String> {
    debug!(target: "http", "POST {} ({} bytes)", url, body.len());

    let response = set_headers!(
        AGENT.post(url).header("Content-Type", "application/json"),
        headers
    )
    .send(body.as_bytes())
    .map_err(|e| anyhow::Error::from(map_transport_error(e)))?;

    let status = response.status().as_u16();
    debug!(target: "http", "POST {} -> {}", url, status);

    let response = check_status(status, url, "POST", response)?;

    response
        .into_body()
        .read_to_string()
        .map_err(|e| anyhow::Error::from(map_transport_error(e)))
        .with_context(|| format!("Failed to read response body from POST {url}"))
}

/// Perform an HTTP POST with a form-urlencoded body.
///
/// 4xx/5xx → `HttpError::HttpStatus`，超时 → `HttpError::Timeout`
pub fn post_form(url: &str, headers: &[&str], body: &str) -> Result<String> {
    debug!(target: "http", "POST {} (form, {} bytes)", url, body.len());

    let response = set_headers!(
        AGENT
            .post(url)
            .header("Content-Type", "application/x-www-form-urlencoded"),
        headers
    )
    .send(body.as_bytes())
    .map_err(|e| anyhow::Error::from(map_transport_error(e)))?;

    let status = response.status().as_u16();
    debug!(target: "http", "POST {} -> {}", url, status);

    let response = check_status(status, url, "POST", response)?;

    response
        .into_body()
        .read_to_string()
        .map_err(|e| anyhow::Error::from(map_transport_error(e)))
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
