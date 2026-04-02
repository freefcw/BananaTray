use super::parse_strategy::{ApiParseStrategy, ParseStrategy};
use crate::models::RefreshData;
use crate::providers::ProviderError;
use anyhow::{Context, Result};
use log::{debug, info, warn};
use regex::Regex;
use std::process::Command;
use std::sync::LazyLock;
use ureq::Agent;

/// 通过 pgrep 匹配的语言服务器进程名（兼容 Intel/ARM）。
const PROCESS_NAMES: &[&str] = &["language_server_macos_arm", "language_server_macos"];

/// Antigravity 本地 language server 的 GetUserStatus API 路径。
const API_PATH: &str = "exa.language_server_pb.LanguageServerService/GetUserStatus";

static CSRF_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"--csrf_token\s+(\S+)").unwrap());
static EXT_PORT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"--extension_server_port\s+(\d+)").unwrap());
static LISTEN_PORT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":(\d+)\s+\(LISTEN\)").unwrap());

/// 允许访问本地自签 HTTPS。
static INSECURE_AGENT: LazyLock<Agent> = LazyLock::new(|| {
    let tls = ureq::tls::TlsConfig::builder()
        .disable_verification(true)
        .build();
    Agent::new_with_config(
        ureq::config::Config::builder()
            .tls_config(tls)
            .http_status_as_error(false)
            .build(),
    )
});

/// 普通 HTTP 请求 Agent。
static PLAIN_AGENT: LazyLock<Agent> = LazyLock::new(|| {
    Agent::new_with_config(
        ureq::config::Config::builder()
            .http_status_as_error(false)
            .build(),
    )
});

/// 从运行中的 language server 进程提取出的连接信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessInfo {
    pub pid: String,
    pub csrf_token: String,
    pub extension_port: Option<u16>,
}

pub fn is_available() -> bool {
    detect_process().is_ok()
}

pub fn fetch_refresh_data() -> Result<RefreshData> {
    let process = detect_process()?;
    let port = resolve_port(&process)?;

    info!(
        target: "providers",
        "Antigravity: fetching user status from port {} (ext: {:?})",
        port, process.extension_port
    );

    let body = fetch_user_status(port, &process.csrf_token, process.extension_port)?;
    let strategy = ApiParseStrategy;
    let (quotas, email, plan_name) = strategy.parse(body.as_bytes())?;
    Ok(RefreshData::with_account(quotas, email, plan_name))
}

pub fn detect_process() -> Result<ProcessInfo> {
    let output = Command::new("/usr/bin/pgrep")
        .args(["-lf", "language_server_macos"])
        .output()
        .map_err(|_| ProviderError::unavailable("pgrep not available"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    if stdout.trim().is_empty() {
        return Err(ProviderError::unavailable("Antigravity language server not running").into());
    }

    stdout
        .lines()
        .find(|line| PROCESS_NAMES.iter().any(|name| line.contains(name)))
        .ok_or_else(|| ProviderError::unavailable("Antigravity language server not running")) // pgrep 有结果但不是目标进程
        .and_then(parse_process_line)
        .map_err(Into::into)
}

pub fn resolve_port(process: &ProcessInfo) -> Result<u16> {
    match discover_port(&process.pid) {
        Ok(port) => Ok(port),
        Err(err) => {
            warn!(
                target: "providers",
                "Antigravity lsof port discovery failed: {}, using extension_port fallback",
                err
            );
            process.extension_port.ok_or_else(|| {
                ProviderError::unavailable("cannot determine Antigravity API port").into()
            })
        }
    }
}

fn parse_process_line(line: &str) -> Result<ProcessInfo, ProviderError> {
    let pid = line
        .split_whitespace()
        .next()
        .ok_or_else(|| ProviderError::parse_failed("no PID in pgrep output"))?
        .to_string();

    let csrf_token = CSRF_RE
        .captures(line)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .ok_or_else(|| ProviderError::parse_failed("--csrf_token not found in process args"))?;

    let extension_port = EXT_PORT_RE
        .captures(line)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<u16>().ok());

    debug!(
        target: "providers",
        "Antigravity process found: pid={}, extension_port={:?}",
        pid, extension_port
    );

    Ok(ProcessInfo {
        pid,
        csrf_token,
        extension_port,
    })
}

fn discover_port(pid: &str) -> Result<u16> {
    let output = Command::new("/usr/sbin/lsof")
        .args(["-nP", "-iTCP", "-sTCP:LISTEN", "-a", "-p", pid])
        .output()
        .map_err(|_| ProviderError::unavailable("lsof not available"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_listen_port(&stdout).ok_or_else(|| {
        ProviderError::parse_failed("no TCP LISTEN port found via lsof for Antigravity").into()
    })
}

fn parse_listen_port(output: &str) -> Option<u16> {
    for captures in LISTEN_PORT_RE.captures_iter(output) {
        if let Some(port) = captures.get(1).and_then(|m| m.as_str().parse::<u16>().ok()) {
            debug!(target: "providers", "Antigravity listen port: {}", port);
            return Some(port);
        }
    }
    None
}

fn fetch_user_status(port: u16, csrf_token: &str, extension_port: Option<u16>) -> Result<String> {
    let body = r#"{"metadata":{"ideName":"antigravity"}}"#;

    for endpoint in build_endpoint_candidates(port, extension_port) {
        match post_api(&endpoint.url, csrf_token, body, endpoint.allow_insecure_tls) {
            Ok(response) => return Ok(response),
            Err(err) => {
                debug!(
                    target: "providers",
                    "Antigravity endpoint failed ({}): {}",
                    endpoint.url,
                    err
                );
            }
        }
    }

    Err(
        ProviderError::fetch_failed("Antigravity API request failed on all candidate endpoints")
            .into(),
    )
}

fn post_api(url: &str, csrf_token: &str, body: &str, allow_insecure_tls: bool) -> Result<String> {
    debug!(target: "providers", "Antigravity POST {}", url);

    let agent = if allow_insecure_tls {
        &*INSECURE_AGENT
    } else {
        &*PLAIN_AGENT
    };

    let response = agent
        .post(url)
        .header("Content-Type", "application/json")
        .header("X-Codeium-Csrf-Token", csrf_token)
        .header("Connect-Protocol-Version", "1")
        .send(body.as_bytes())
        .with_context(|| format!("POST {} failed", url))?;

    let status = response.status().as_u16();
    debug!(target: "providers", "Antigravity POST {} -> {}", url, status);

    if status >= 400 {
        anyhow::bail!("Antigravity API returned status {}", status);
    }

    response
        .into_body()
        .read_to_string()
        .with_context(|| format!("Failed to read Antigravity API response from {}", url))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EndpointCandidate {
    url: String,
    allow_insecure_tls: bool,
}

fn build_endpoint_candidates(port: u16, extension_port: Option<u16>) -> Vec<EndpointCandidate> {
    let mut endpoints = vec![EndpointCandidate {
        url: format!("https://127.0.0.1:{}/{}", port, API_PATH),
        allow_insecure_tls: true,
    }];

    if let Some(extension_port) = extension_port {
        endpoints.push(EndpointCandidate {
            url: format!("http://127.0.0.1:{}/{}", extension_port, API_PATH),
            allow_insecure_tls: false,
        });
    }

    endpoints.push(EndpointCandidate {
        url: format!("http://127.0.0.1:{}/{}", port, API_PATH),
        allow_insecure_tls: false,
    });

    endpoints
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_process_line_success() {
        let line =
            "12345 language_server_macos_arm --csrf_token abc123 --extension_server_port 4242";
        let process = parse_process_line(line).unwrap();

        assert_eq!(process.pid, "12345");
        assert_eq!(process.csrf_token, "abc123");
        assert_eq!(process.extension_port, Some(4242));
    }

    #[test]
    fn test_parse_process_line_requires_csrf_token() {
        let line = "12345 language_server_macos_arm --extension_server_port 4242";
        let err = parse_process_line(line).unwrap_err();
        assert!(matches!(err, ProviderError::ParseFailed { .. }));
    }

    #[test]
    fn test_parse_listen_port_returns_first_match() {
        let output = "\
COMMAND   PID USER   FD   TYPE             DEVICE SIZE/OFF NODE NAME
server  12345 user   10u  IPv4 0x01             0t0  TCP *:51234 (LISTEN)
server  12345 user   11u  IPv4 0x02             0t0  TCP *:51235 (LISTEN)";

        assert_eq!(parse_listen_port(output), Some(51234));
    }

    #[test]
    fn test_build_endpoint_candidates_order() {
        let urls = build_endpoint_candidates(8443, Some(3000));
        assert_eq!(urls.len(), 3);
        assert_eq!(
            urls[0].url,
            "https://127.0.0.1:8443/exa.language_server_pb.LanguageServerService/GetUserStatus"
        );
        assert!(urls[0].allow_insecure_tls);
        assert_eq!(
            urls[1].url,
            "http://127.0.0.1:3000/exa.language_server_pb.LanguageServerService/GetUserStatus"
        );
        assert_eq!(
            urls[2].url,
            "http://127.0.0.1:8443/exa.language_server_pb.LanguageServerService/GetUserStatus"
        );
    }
}
