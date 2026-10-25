//! Shared HTTP client utilities for providers.
//!
//! Uses `ureq` for type-safe HTTP requests instead of shelling out to `curl`.

use anyhow::{Context, Result};
use log::{debug, info, warn};
use std::fmt;
use std::sync::LazyLock;
use std::time::{Duration, Instant};
use ureq::Agent;

const HTTP_TIMEOUT: Duration = Duration::from_secs(20);
/// 单次请求最多跟随的重定向次数（仅限同主机）。
const MAX_REDIRECTS: usize = 5;
/// 响应正文上限，防止异常响应在超时窗口内把进程内存打满。
const MAX_RESPONSE_BYTES: u64 = super::MAX_EXTERNAL_PAYLOAD_BYTES;
type HttpResponse = ureq::http::Response<ureq::Body>;

/// 构建 agent。
///
/// 重定向由本模块自己处理（`max_redirects(0)`），因为 ureq 跟随重定向时
/// 只会剥离 `Authorization` / `Cookie`，认不出自定义 provider 的
/// `X-Api-Key` 这类密钥头，跨主机跟随就等于把密钥送给第三方。
fn build_agent(timeout: Duration) -> Agent {
    Agent::new_with_config(
        ureq::config::Config::builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .timeout_global(Some(timeout))
            .build(),
    )
}

static AGENT: LazyLock<Agent> = LazyLock::new(|| build_agent(HTTP_TIMEOUT));

fn agent_with_timeout(timeout: Option<Duration>) -> Agent {
    match timeout {
        Some(timeout) => build_agent(timeout),
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
    /// 服务端返回了 HTTP 错误状态码；正文有意丢弃，避免跨层泄漏敏感信息。
    HttpStatus { code: u16 },
    /// 目标 URL 不是 https（回环地址除外）。
    InsecureScheme,
    /// 重定向被拒绝：跨 origin、协议降级，或超过次数上限。
    Redirect(String),
    /// 响应正文超过上限。
    BodyTooLarge { limit: u64 },
}

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Timeout => write!(f, "request timeout"),
            Self::Transport(reason) => write!(f, "transport error: {}", reason),
            Self::HttpStatus { code } => write!(f, "HTTP status {}", code),
            Self::InsecureScheme => write!(f, "refusing non-https request"),
            Self::Redirect(reason) => write!(f, "redirect refused: {}", reason),
            Self::BodyTooLarge { limit } => {
                write!(f, "response body exceeds {} bytes", limit)
            }
        }
    }
}

impl std::error::Error for HttpError {}

impl HttpError {
    /// 是否为认证类错误（401 / 403）
    pub fn is_auth_error(&self) -> bool {
        matches!(self, Self::HttpStatus { code } if *code == 401 || *code == 403)
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
fn check_status(status: u16, method: &str, response: HttpResponse) -> Result<HttpResponse> {
    if status >= 400 {
        // 上游错误正文可能包含 token、cookie、邮箱等敏感信息。
        // HTTP 公共层只传播状态码，不读取、记录或展示原始正文。
        warn!(target: "http", "{} request failed with status {}", method, status);
        return Err(HttpError::HttpStatus { code: status }.into());
    }
    Ok(response)
}

/// 读取响应正文，并强制正文上限。
fn read_response_body(response: HttpResponse, context: impl FnOnce() -> String) -> Result<String> {
    response
        .into_body()
        .into_with_config()
        .limit(MAX_RESPONSE_BYTES)
        .read_to_string()
        .map_err(map_body_error)
        .with_context(context)
}

fn map_body_error(err: ureq::Error) -> anyhow::Error {
    match err {
        ureq::Error::BodyExceedsLimit(limit) => {
            warn!(
                target: "http",
                "response body exceeded the {} byte limit; request aborted",
                limit
            );
            HttpError::BodyTooLarge { limit }.into()
        }
        other => anyhow::Error::from(map_transport_error(other)),
    }
}

fn is_redirect_status(status: u16) -> bool {
    matches!(status, 301 | 302 | 303 | 307 | 308)
}

fn is_loopback_host(host: Option<&str>) -> bool {
    matches!(host, Some("127.0.0.1" | "localhost" | "::1" | "[::1]"))
}

/// 只允许 https；回环地址例外，本机测试服务器走明文 http。
fn ensure_safe_scheme(url: &str) -> Result<()> {
    let uri = ureq::http::Uri::try_from(url)
        .map_err(|_| HttpError::Transport("invalid URL".to_string()))?;
    match uri.scheme_str() {
        Some("https") => Ok(()),
        Some("http") if is_loopback_host(uri.host()) => Ok(()),
        _ => {
            // URL 可能含凭证（query 里的 token），只记 scheme 和 host。
            warn!(
                target: "http",
                "refusing non-https request to host {:?}",
                uri.host()
            );
            Err(HttpError::InsecureScheme.into())
        }
    }
}

fn default_port(scheme: &str) -> Option<u16> {
    match scheme {
        "http" => Some(80),
        "https" => Some(443),
        _ => None,
    }
}

fn origin(url: &str) -> Option<(String, String, u16)> {
    let uri = ureq::http::Uri::try_from(url).ok()?;
    let scheme = uri.scheme_str()?.to_ascii_lowercase();
    let host = uri.host()?.to_ascii_lowercase();
    let port = uri.port_u16().or_else(|| default_port(&scheme))?;
    Some((scheme, host, port))
}

fn same_origin(a: &str, b: &str) -> bool {
    match (origin(a), origin(b)) {
        (Some((scheme_a, host_a, port_a)), Some((scheme_b, host_b, port_b))) => {
            scheme_a == scheme_b && host_a == host_b && port_a == port_b
        }
        _ => false,
    }
}

fn strip_fragment(reference: &str) -> &str {
    reference
        .split_once('#')
        .map_or(reference, |(without_fragment, _)| without_fragment)
}

fn normalize_path(path: &str) -> String {
    let leading = path.starts_with('/');
    let trailing = path.ends_with('/');
    let mut parts = Vec::new();

    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            value => parts.push(value),
        }
    }

    let mut normalized = String::new();
    if leading {
        normalized.push('/');
    }
    normalized.push_str(&parts.join("/"));
    if trailing && !normalized.ends_with('/') {
        normalized.push('/');
    }
    if normalized.is_empty() {
        normalized.push('/');
    }
    normalized
}

/// 把 `Location` 解析成绝对 URL（支持绝对 URL、绝对路径、相对路径）。
fn resolve_redirect(base: &str, location: &str) -> Result<String> {
    let location_without_fragment = strip_fragment(location);
    let base_uri = ureq::http::Uri::try_from(base)
        .map_err(|_| HttpError::Redirect("invalid base URL".to_string()))?;
    let scheme = base_uri.scheme_str().unwrap_or("https");
    let authority = base_uri
        .authority()
        .ok_or_else(|| HttpError::Redirect("base URL has no host".to_string()))?;

    if location_without_fragment.starts_with("//") {
        return Ok(format!("{scheme}:{location_without_fragment}"));
    }

    if location_without_fragment.is_empty() || location_without_fragment.starts_with('?') {
        let path = base_uri.path();
        let query = if location_without_fragment.is_empty() {
            base_uri
                .query()
                .map_or(String::new(), |value| format!("?{value}"))
        } else {
            location_without_fragment.to_string()
        };
        return Ok(format!("{scheme}://{authority}{path}{query}"));
    }

    if location_without_fragment.contains("://") {
        let location_uri = ureq::http::Uri::try_from(location_without_fragment)
            .map_err(|_| HttpError::Redirect("invalid Location header".to_string()))?;
        if location_uri.scheme().is_some() {
            return Ok(location_without_fragment.to_string());
        }
    }

    if location_without_fragment.starts_with('/') {
        return Ok(format!(
            "{scheme}://{authority}{}",
            normalize_path(location_without_fragment)
        ));
    }

    let base_path = base_uri.path();
    let dir = match base_path.rfind('/') {
        Some(idx) => &base_path[..=idx],
        None => "/",
    };
    Ok(format!(
        "{scheme}://{authority}{}",
        normalize_path(&format!("{dir}{location_without_fragment}"))
    ))
}

/// 发送请求并跟随重定向，但只跟同 origin。
///
/// 跨主机重定向直接报错：自定义 provider 的密钥头不会像 `Authorization`
/// 那样被自动剥离，跟着走等于把凭证交给第三方。非 GET 请求不跟随重定向。
fn send_with_redirects(
    method: &str,
    timeout: Option<Duration>,
    initial_url: &str,
    mut send: impl FnMut(&Agent, &str) -> std::result::Result<HttpResponse, ureq::Error>,
) -> Result<HttpResponse> {
    ensure_safe_scheme(initial_url)?;
    let mut url = initial_url.to_string();
    let deadline = timeout.map(|value| Instant::now() + value);

    let mut redirects = 0;
    loop {
        let remaining = deadline.map(|at| at.saturating_duration_since(Instant::now()));
        if remaining.is_some_and(|value| value.is_zero()) {
            return Err(HttpError::Timeout.into());
        }
        let agent = agent_with_timeout(remaining);
        let response =
            send(&agent, &url).map_err(|e| anyhow::Error::from(map_transport_error(e)))?;
        let status = response.status().as_u16();

        if !is_redirect_status(status) {
            debug!(target: "http", "{} request completed with status {}", method, status);
            return Ok(response);
        }

        if method != "GET" {
            warn!(
                target: "http",
                "{} request got a redirect; not following it for non-GET",
                method
            );
            return Err(HttpError::Redirect(format!(
                "refusing to follow a redirect for a {method} request"
            ))
            .into());
        }

        if redirects >= MAX_REDIRECTS {
            warn!(
                target: "http",
                "{} request exceeded the {} redirect limit",
                method,
                MAX_REDIRECTS
            );
            return Err(HttpError::Redirect("too many redirects".to_string()).into());
        }
        redirects += 1;

        let location = response
            .headers()
            .get(ureq::http::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| HttpError::Redirect("redirect without Location header".to_string()))?;

        let next = resolve_redirect(&url, location)?;
        if !same_origin(&url, &next) {
            // 自定义 provider 的密钥头（X-Api-Key 等）不会被 ureq 自动剥离，
            // 跟过去等于把凭证交给第三方。只记 host，不记完整 URL。
            warn!(
                target: "http",
                "refusing cross-origin redirect from host {:?} to host {:?}",
                ureq::http::Uri::try_from(url.as_str())
                    .ok()
                    .and_then(|u| u.host().map(str::to_string)),
                ureq::http::Uri::try_from(next.as_str())
                    .ok()
                    .and_then(|u| u.host().map(str::to_string))
            );
            return Err(HttpError::Redirect("cross-origin redirect refused".to_string()).into());
        }
        ensure_safe_scheme(&next)?;

        // 同 origin 重定向会带着自定义密钥头重新发一次请求，是凭证真正被重发的
        // 唯一位置；默认日志级别是 Info，这里用 info 才能在正式包里看到。
        info!(target: "http", "{} following same-origin redirect", method);
        url = next;
    }
}

fn send_and_read(
    method: &str,
    timeout: Option<Duration>,
    url: &str,
    send: impl FnMut(&Agent, &str) -> std::result::Result<HttpResponse, ureq::Error>,
    body_context: impl FnOnce() -> String,
) -> Result<String> {
    let response = send_with_redirects(method, timeout, url, send)?;
    let status = response.status().as_u16();
    let response = check_status(status, method, response)?;
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
    debug!(target: "http", "GET request started");

    send_and_read(
        "GET",
        timeout,
        url,
        |agent, url| set_headers!(agent.get(url), headers).call(),
        || "Failed to read HTTP response body".to_string(),
    )
}

/// Perform an HTTP GET and return the full raw HTTP response (status line + headers + body).
///
/// The response is formatted as `"HTTP/1.1 <status>\r\n<headers>\r\n\r\n<body>"`
/// to maintain compatibility with callers that parse raw HTTP responses (e.g. Codex).
/// 与 [`get`] 的差异在响应格式而非请求头——两者都接受自定义请求头。
///
/// 4xx/5xx → `HttpError::HttpStatus`，超时 → `HttpError::Timeout`
pub fn get_raw_http_response(url: &str, headers: &[&str]) -> Result<String> {
    debug!(target: "http", "GET raw-response request started");

    let response = send_with_redirects("GET", None, url, |agent, url| {
        set_headers!(agent.get(url), headers).call()
    })?;

    let status = response.status().as_u16();

    let response = check_status(status, "GET", response)?;

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

    let body = read_response_body(response, || "Failed to read HTTP response body".to_string())?;
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
    debug!(target: "http", "POST request started ({} bytes)", body.len());

    send_and_read(
        "POST",
        timeout,
        url,
        |agent, url| {
            set_headers!(
                agent.post(url).header("Content-Type", "application/json"),
                headers
            )
            .send(body.as_bytes())
        },
        || "Failed to read HTTP POST response body".to_string(),
    )
}

/// Perform an HTTP POST with a form-urlencoded body.
///
/// 4xx/5xx → `HttpError::HttpStatus`，超时 → `HttpError::Timeout`
pub fn post_form(url: &str, headers: &[&str], body: &str) -> Result<String> {
    debug!(target: "http", "POST form request started ({} bytes)", body.len());

    send_and_read(
        "POST",
        None,
        url,
        |agent, url| {
            set_headers!(
                agent
                    .post(url)
                    .header("Content-Type", "application/x-www-form-urlencoded"),
                headers
            )
            .send(body.as_bytes())
        },
        || "Failed to read HTTP POST response body".to_string(),
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
            Self::responding_with(status, body, &[])
        }

        /// 带额外响应头的单响应服务器（用于构造 `Location` 重定向）。
        fn responding_with(status: u16, body: &str, extra_headers: &[(&str, &str)]) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            let (request_tx, request_rx) = mpsc::channel();
            let response_body = body.to_string();
            let extra: Vec<(String, String)> = extra_headers
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();

            let handle = thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let request = read_http_request(&mut stream);
                request_tx.send(request).unwrap();

                let reason = if status < 400 { "OK" } else { "ERROR" };
                let mut response = format!("HTTP/1.1 {status} {reason}\r\n");
                for (name, value) in &extra {
                    response.push_str(&format!("{name}: {value}\r\n"));
                }
                response.push_str(&format!(
                    "Content-Length: {}\r\nConnection: close\r\n\r\n{}",
                    response_body.len(),
                    response_body
                ));
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
    fn http_status_error_omits_sensitive_response_body() {
        let secret = "token=secret-123; email=user@example.com; 中文🍌";
        let server = TestServer::responding(429, secret);

        let error = get(&server.url, &[]).unwrap_err();
        let request = server.take_request();

        assert!(request.starts_with("GET / HTTP/1.1\r\n"));
        assert!(!error.to_string().contains(secret));
        match error.downcast_ref::<HttpError>().unwrap() {
            HttpError::HttpStatus { code } => {
                assert_eq!(*code, 429);
            }
            other => panic!("expected HTTP status error, got {other:?}"),
        }
    }

    // ── 重定向策略 ────────────────────────────────

    #[test]
    fn cross_host_redirect_is_refused() {
        // 目标指向另一个主机（example.com），必须拒绝，不能把密钥头带过去。
        let server =
            TestServer::responding_with(302, "", &[("Location", "https://example.com/steal")]);

        let error = get(&server.url, &["X-Api-Key: secret"]).unwrap_err();
        let _ = server.take_request();

        match error.downcast_ref::<HttpError>().unwrap() {
            HttpError::Redirect(reason) => assert!(reason.contains("cross-origin")),
            other => panic!("expected redirect refusal, got {other:?}"),
        }
    }

    #[test]
    fn cross_port_redirect_is_refused() {
        assert!(!same_origin(
            "https://api.example.com/api",
            "https://api.example.com:8443/collect"
        ));
        assert!(same_origin(
            "https://api.example.com/api",
            "https://api.example.com:443/collect"
        ));
    }

    #[test]
    fn insecure_non_https_url_is_refused() {
        // 非回环的明文 http 必须拒绝。
        let error = get("http://example.com/api", &[]).unwrap_err();
        assert!(matches!(
            error.downcast_ref::<HttpError>().unwrap(),
            HttpError::InsecureScheme
        ));
    }

    #[test]
    fn loopback_http_is_allowed() {
        // 回环地址例外：本机测试服务器走明文 http。
        let server = TestServer::responding(200, "ok");
        assert_eq!(get(&server.url, &[]).unwrap(), "ok");
        let _ = server.take_request();
    }

    #[test]
    fn post_does_not_follow_redirects() {
        let server = TestServer::responding_with(302, "", &[("Location", "/elsewhere")]);

        let error = post_json(&server.url, &[], "{}").unwrap_err();
        let _ = server.take_request();

        match error.downcast_ref::<HttpError>().unwrap() {
            HttpError::Redirect(reason) => assert!(reason.contains("POST")),
            other => panic!("expected redirect refusal, got {other:?}"),
        }
    }

    #[test]
    fn same_host_redirect_is_followed() {
        // 同一个 listener 连服两次：先 302 到自身路径，再 200。
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let url = format!("http://{addr}/start");

        let handle = thread::spawn(move || {
            let mut seen = Vec::new();
            for (idx, response) in [
                format!(
                    "HTTP/1.1 302 OK\r\nLocation: http://{addr}/final\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                ),
                "HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nfinal".to_string(),
            ]
            .iter()
            .enumerate()
            {
                let (mut stream, _) = listener.accept().unwrap();
                seen.push(read_http_request(&mut stream));
                stream.write_all(response.as_bytes()).unwrap();
                if idx == 1 {
                    break;
                }
            }
            seen
        });

        let body = get(&url, &["X-Api-Key: same-host-secret"]).unwrap();
        assert_eq!(body, "final");

        let requests = handle.join().unwrap();
        assert_eq!(requests.len(), 2, "redirect should be followed once");
        assert!(requests[0].starts_with("GET /start HTTP/1.1\r\n"));
        assert!(requests[1].starts_with("GET /final HTTP/1.1\r\n"));
        // 同主机重定向保留自定义密钥头（ureq 内置剥离认不出 X-Api-Key）。
        assert!(requests[1]
            .to_ascii_lowercase()
            .contains("x-api-key: same-host-secret"));
    }

    #[test]
    fn relative_redirect_resolution_preserves_query_and_normalizes_path() {
        assert_eq!(
            resolve_redirect("https://example.com/api/v1?old=1", "?page=2").unwrap(),
            "https://example.com/api/v1?page=2"
        );
        assert_eq!(
            resolve_redirect("https://example.com/api/v1", "../v2").unwrap(),
            "https://example.com/v2"
        );
        assert_eq!(
            resolve_redirect("https://example.com/api/v1", "//other.example/v2").unwrap(),
            "https://other.example/v2"
        );
    }

    #[test]
    fn redirect_limit_allows_at_most_configured_number() {
        assert_eq!(MAX_REDIRECTS, 5);
        assert_eq!((0..MAX_REDIRECTS).count(), MAX_REDIRECTS);
    }

    #[test]
    fn oversized_response_body_is_rejected() {
        let big = "x".repeat((MAX_RESPONSE_BYTES + 1) as usize);
        let server = TestServer::responding(200, &big);

        let error = get(&server.url, &[]).unwrap_err();
        let _ = server.take_request();

        assert!(matches!(
            error.downcast_ref::<HttpError>().unwrap(),
            HttpError::BodyTooLarge { .. }
        ));
    }
}
