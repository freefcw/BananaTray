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
type HttpResponse = ureq::http::Response<ureq::Body>;

static AGENT: LazyLock<Agent> = LazyLock::new(|| {
    Agent::new_with_config(
        ureq::config::Config::builder()
            .http_status_as_error(false)
            .timeout_global(Some(HTTP_TIMEOUT))
            .build(),
    )
});

fn agent_with_timeout(timeout: Option<Duration>) -> Agent {
    match timeout {
        Some(timeout) => Agent::new_with_config(
            ureq::config::Config::builder()
                .http_status_as_error(false)
                .timeout_global(Some(timeout))
                .build(),
        ),
        None => AGENT.clone(),
    }
}

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
    response: HttpResponse,
) -> Result<HttpResponse> {
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

fn read_response_body(response: HttpResponse, context: impl FnOnce() -> String) -> Result<String> {
    response
        .into_body()
        .read_to_string()
        .map_err(|e| anyhow::Error::from(map_transport_error(e)))
        .with_context(context)
}

fn send_and_read(
    method: &str,
    url: &str,
    timeout: Option<Duration>,
    send: impl FnOnce(&Agent) -> std::result::Result<HttpResponse, ureq::Error>,
    body_context: impl FnOnce() -> String,
) -> Result<String> {
    let agent = agent_with_timeout(timeout);
    let response = send(&agent).map_err(|e| anyhow::Error::from(map_transport_error(e)))?;

    let status = response.status().as_u16();
    debug!(target: "http", "{} {} -> {}", method, url, status);

    let response = check_status(status, url, method, response)?;
    read_response_body(response, body_context)
}

/// Perform an HTTP GET and return the response body as a String.
///
/// `headers` is a list of header strings like `"Authorization: Bearer xxx"`.
///
/// 4xx/5xx → `HttpError::HttpStatus`，超时 → `HttpError::Timeout`
#[allow(dead_code)]
pub fn get(url: &str, headers: &[&str]) -> Result<String> {
    get_with_timeout(url, headers, None)
}

/// Perform an HTTP GET with an optional per-request timeout.
pub fn get_with_timeout(url: &str, headers: &[&str], timeout: Option<Duration>) -> Result<String> {
    debug!(target: "http", "GET {}", url);

    send_and_read(
        "GET",
        url,
        timeout,
        |agent| set_headers!(agent.get(url), headers).call(),
        || format!("Failed to read response body from {url}"),
    )
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

    let body = read_response_body(response, || {
        format!("Failed to read response body from {url}")
    })?;
    raw.push_str(&body);

    Ok(raw)
}

/// Perform an HTTP POST with a JSON body (Content-Type: application/json).
///
/// 4xx/5xx → `HttpError::HttpStatus`，超时 → `HttpError::Timeout`
pub fn post_json(url: &str, headers: &[&str], body: &str) -> Result<String> {
    post_json_with_timeout(url, headers, body, None)
}

/// Perform an HTTP POST with a JSON body and an optional per-request timeout.
pub fn post_json_with_timeout(
    url: &str,
    headers: &[&str],
    body: &str,
    timeout: Option<Duration>,
) -> Result<String> {
    debug!(target: "http", "POST {} ({} bytes)", url, body.len());

    send_and_read(
        "POST",
        url,
        timeout,
        |agent| {
            set_headers!(
                agent.post(url).header("Content-Type", "application/json"),
                headers
            )
            .send(body.as_bytes())
        },
        || format!("Failed to read response body from POST {url}"),
    )
}

/// Perform an HTTP POST with a form-urlencoded body.
///
/// 4xx/5xx → `HttpError::HttpStatus`，超时 → `HttpError::Timeout`
pub fn post_form(url: &str, headers: &[&str], body: &str) -> Result<String> {
    debug!(target: "http", "POST {} (form, {} bytes)", url, body.len());

    send_and_read(
        "POST",
        url,
        None,
        |agent| {
            set_headers!(
                agent
                    .post(url)
                    .header("Content-Type", "application/x-www-form-urlencoded"),
                headers
            )
            .send(body.as_bytes())
        },
        || format!("Failed to read response body from POST {url}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::mpsc;
    use std::thread;

    struct TestServer {
        url: String,
        request_rx: mpsc::Receiver<String>,
        handle: thread::JoinHandle<()>,
    }

    impl TestServer {
        fn responding(status: u16, body: &str) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            let (request_tx, request_rx) = mpsc::channel();
            let response_body = body.to_string();

            let handle = thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let request = read_http_request(&mut stream);
                request_tx.send(request).unwrap();

                let reason = if status < 400 { "OK" } else { "ERROR" };
                let response = format!(
                    "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    response_body.len(),
                    response_body
                );
                stream.write_all(response.as_bytes()).unwrap();
            });

            Self {
                url,
                request_rx,
                handle,
            }
        }

        fn take_request(self) -> String {
            let request = self
                .request_rx
                .recv_timeout(std::time::Duration::from_secs(2))
                .unwrap();
            self.handle.join().unwrap();
            request
        }
    }

    fn read_http_request(stream: &mut TcpStream) -> String {
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();

        let mut buffer = Vec::new();
        let mut chunk = [0; 512];
        let header_end = loop {
            let bytes_read = stream.read(&mut chunk).unwrap();
            assert_ne!(bytes_read, 0, "client closed before request headers");
            buffer.extend_from_slice(&chunk[..bytes_read]);

            if let Some(header_end) = find_header_end(&buffer) {
                break header_end;
            }
        };

        let content_length = request_content_length(&buffer[..header_end]);
        let request_len = header_end + 4 + content_length;
        while buffer.len() < request_len {
            let bytes_read = stream.read(&mut chunk).unwrap();
            assert_ne!(bytes_read, 0, "client closed before request body");
            buffer.extend_from_slice(&chunk[..bytes_read]);
        }

        String::from_utf8_lossy(&buffer).into_owned()
    }

    fn find_header_end(buffer: &[u8]) -> Option<usize> {
        buffer.windows(4).position(|window| window == b"\r\n\r\n")
    }

    fn request_content_length(headers: &[u8]) -> usize {
        let headers = String::from_utf8_lossy(headers);
        headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse().unwrap())
            })
            .unwrap_or(0)
    }

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

    #[test]
    fn get_reads_success_response_body() {
        let server = TestServer::responding(200, "hello");

        let body = get(&server.url, &["X-Test: yes"]).unwrap();
        let request = server.take_request();

        assert_eq!(body, "hello");
        assert!(request.starts_with("GET / HTTP/1.1\r\n"));
        assert!(request.to_ascii_lowercase().contains("x-test: yes"));
    }

    #[test]
    fn post_json_sends_json_body_and_reads_response_body() {
        let server = TestServer::responding(200, "created");

        let body = post_json(&server.url, &["X-Test: yes"], r#"{"ok":true}"#).unwrap();
        let request = server.take_request();
        let request_lower = request.to_ascii_lowercase();

        assert_eq!(body, "created");
        assert!(request.starts_with("POST / HTTP/1.1\r\n"));
        assert!(request_lower.contains("content-type: application/json"));
        assert!(request_lower.contains("x-test: yes"));
        assert!(request.ends_with("\r\n\r\n{\"ok\":true}"));
    }

    #[test]
    fn post_form_sends_form_body_and_reads_response_body() {
        let server = TestServer::responding(200, "accepted");

        let body = post_form(&server.url, &[], "a=1&b=two").unwrap();
        let request = server.take_request();
        let request_lower = request.to_ascii_lowercase();

        assert_eq!(body, "accepted");
        assert!(request.starts_with("POST / HTTP/1.1\r\n"));
        assert!(request_lower.contains("content-type: application/x-www-form-urlencoded"));
        assert!(request.ends_with("\r\n\r\na=1&b=two"));
    }

    #[test]
    fn http_status_error_includes_response_body() {
        let server = TestServer::responding(429, "rate limited");

        let error = get(&server.url, &[]).unwrap_err();
        let request = server.take_request();

        assert!(request.starts_with("GET / HTTP/1.1\r\n"));
        match error.downcast_ref::<HttpError>().unwrap() {
            HttpError::HttpStatus { code, body } => {
                assert_eq!(*code, 429);
                assert_eq!(body, "rate limited");
            }
            other => panic!("expected HTTP status error, got {other:?}"),
        }
    }
}
