use super::parse_strategy::{ApiParseStrategy, ParseStrategy};
use super::spec::CodeiumFamilySpec;
use super::LOCAL_API_SOURCE_LABEL;
use crate::models::RefreshData;
use crate::providers::{ProviderError, ProviderResult};
use log::{debug, info, warn};
use regex::Regex;
use std::process::Command;
use std::sync::LazyLock;
use ureq::Agent;

pub(super) const PROCESS_QUERY: &str = "language_server_";

const PROCESS_NAMES: &[&str] = &[
    "language_server_macos_arm",
    "language_server_macos",
    "language_server_linux_x64",
    "language_server_linux_arm64",
];
const LSOF_CANDIDATES: &[&str] = &["/usr/sbin/lsof", "/usr/bin/lsof", "lsof"];
const API_PATH: &str = "exa.language_server_pb.LanguageServerService/GetUserStatus";

static CSRF_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"--csrf_token\s+(\S+)").unwrap());
static EXT_PORT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"--extension_server_port\s+(\d+)").unwrap());
static LISTEN_PORT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":(\d+)\s+\(LISTEN\)").unwrap());
static ARG_START_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s--[A-Za-z0-9]").unwrap());

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

static PLAIN_AGENT: LazyLock<Agent> = LazyLock::new(|| {
    Agent::new_with_config(
        ureq::config::Config::builder()
            .http_status_as_error(false)
            .build(),
    )
});

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessInfo {
    pub pid: String,
    pub binary_path: Option<String>,
    /// 新版 Windsurf 不再通过进程参数传递 csrf_token，改用 stdin_initial_metadata
    pub csrf_token: Option<String>,
    pub extension_port: Option<u16>,
}

pub fn is_available(spec: &CodeiumFamilySpec) -> bool {
    detect_process(spec).is_ok()
}

pub fn fetch_refresh_data(spec: &CodeiumFamilySpec) -> ProviderResult<RefreshData> {
    let process = detect_process(spec)?;
    let port = resolve_port(&process, spec)?;

    info!(
        target: "providers",
        "{}: fetching user status from port {} (ext: {:?})",
        spec.log_label,
        port,
        process.extension_port
    );

    let body = fetch_user_status(
        port,
        process.csrf_token.as_deref(),
        process.extension_port,
        spec,
    )?;
    let strategy = ApiParseStrategy;
    let (quotas, email, plan_name) = strategy.parse(body.as_bytes())?;
    Ok(RefreshData::with_account(quotas, email, plan_name)
        .with_source_label(LOCAL_API_SOURCE_LABEL))
}

pub fn detect_process(spec: &CodeiumFamilySpec) -> ProviderResult<ProcessInfo> {
    let output = Command::new("/usr/bin/pgrep")
        .args(["-lf", PROCESS_QUERY])
        .output()
        .map_err(|_| ProviderError::unavailable("pgrep not available"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    if stdout.trim().is_empty() {
        return Err(ProviderError::unavailable(&format!(
            "{} language server not running",
            spec.log_label
        )));
    }

    stdout
        .lines()
        .find(|line| matches_process_line(line, spec))
        .ok_or_else(|| {
            ProviderError::unavailable(&format!("{} language server not running", spec.log_label))
        })
        .and_then(parse_process_line)
}

pub fn matches_process_line(line: &str, spec: &CodeiumFamilySpec) -> bool {
    let lower = line.to_lowercase();

    if !PROCESS_NAMES.iter().any(|name| lower.contains(name)) {
        return false;
    }

    spec.process_markers
        .iter()
        .any(|marker| lower.contains(marker))
}

pub fn resolve_port(process: &ProcessInfo, spec: &CodeiumFamilySpec) -> ProviderResult<u16> {
    match discover_port(&process.pid, spec) {
        Ok(port) => Ok(port),
        Err(err) => {
            warn!(
                target: "providers",
                "{} lsof port discovery failed: {}, using extension_port fallback",
                spec.log_label,
                err
            );
            process.extension_port.ok_or_else(|| {
                ProviderError::unavailable(&format!("cannot determine {} API port", spec.log_label))
            })
        }
    }
}

pub(super) fn parse_process_line(line: &str) -> Result<ProcessInfo, ProviderError> {
    let line = line.trim();
    let Some(first_ws) = line.find(char::is_whitespace) else {
        return Err(ProviderError::parse_failed(
            "no command line in pgrep output",
        ));
    };

    let pid = line[..first_ws].to_string();
    let command_line = line[first_ws..].trim_start();
    let binary_path = extract_binary_path(command_line);

    let csrf_token = CSRF_RE
        .captures(line)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string());

    let extension_port = EXT_PORT_RE
        .captures(line)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<u16>().ok());

    debug!(
        target: "providers",
        "Codeium-family process found: pid={}, extension_port={:?}",
        pid,
        extension_port
    );

    Ok(ProcessInfo {
        pid,
        binary_path,
        csrf_token,
        extension_port,
    })
}

fn extract_binary_path(command_line: &str) -> Option<String> {
    let command_line = command_line.trim();
    if command_line.is_empty() {
        return None;
    }

    if let Some(arg_start) = ARG_START_RE.find(command_line) {
        return Some(command_line[..arg_start.start()].trim_end().to_string());
    }

    Some(command_line.to_string())
}

pub(super) fn discover_port(pid: &str, spec: &CodeiumFamilySpec) -> ProviderResult<u16> {
    let mut last_error = None;

    for command in LSOF_CANDIDATES {
        let output = match Command::new(command)
            .args(["-nP", "-iTCP", "-sTCP:LISTEN", "-a", "-p", pid])
            .output()
        {
            Ok(output) => output,
            Err(err) => {
                last_error = Some(err);
                continue;
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        return parse_listen_port(&stdout).ok_or_else(|| {
            ProviderError::parse_failed(&format!(
                "no TCP LISTEN port found via lsof for {}",
                spec.log_label
            ))
        });
    }

    let message = last_error
        .map(|err| format!("lsof not available: {err}"))
        .unwrap_or_else(|| "lsof not available".to_string());
    Err(ProviderError::unavailable(&message))
}

fn parse_listen_port(output: &str) -> Option<u16> {
    for captures in LISTEN_PORT_RE.captures_iter(output) {
        if let Some(port) = captures.get(1).and_then(|m| m.as_str().parse::<u16>().ok()) {
            debug!(target: "providers", "Codeium-family listen port: {}", port);
            return Some(port);
        }
    }
    None
}

fn fetch_user_status(
    port: u16,
    csrf_token: Option<&str>,
    extension_port: Option<u16>,
    spec: &CodeiumFamilySpec,
) -> ProviderResult<String> {
    let body = format!(r#"{{"metadata":{{"ideName":"{}"}}}}"#, spec.ide_name);

    for endpoint in build_endpoint_candidates(port, extension_port) {
        match post_api(
            &endpoint.url,
            csrf_token,
            &body,
            endpoint.allow_insecure_tls,
            spec,
        ) {
            Ok(response) => return Ok(response),
            Err(err) => {
                debug!(
                    target: "providers",
                    "{} endpoint failed ({}): {}",
                    spec.log_label,
                    endpoint.url,
                    err
                );
            }
        }
    }

    Err(ProviderError::fetch_failed(&format!(
        "{} API request failed on all candidate endpoints",
        spec.log_label
    )))
}

fn post_api(
    url: &str,
    csrf_token: Option<&str>,
    body: &str,
    allow_insecure_tls: bool,
    spec: &CodeiumFamilySpec,
) -> ProviderResult<String> {
    debug!(target: "providers", "{} POST {}", spec.log_label, url);

    let agent = if allow_insecure_tls {
        &*INSECURE_AGENT
    } else {
        &*PLAIN_AGENT
    };

    let mut request = agent
        .post(url)
        .header("Content-Type", "application/json")
        .header("Connect-Protocol-Version", "1");

    if let Some(token) = csrf_token {
        request = request.header("X-Codeium-Csrf-Token", token);
    }

    let response = request
        .send(body.as_bytes())
        .map_err(|err| ProviderError::fetch_failed(&format!("POST {url} failed: {err}")))?;

    let status = response.status().as_u16();
    debug!(target: "providers", "{} POST {} -> {}", spec.log_label, url, status);

    if status >= 400 {
        return Err(ProviderError::fetch_failed(&format!(
            "{} API returned status {}",
            spec.log_label, status
        )));
    }

    response.into_body().read_to_string().map_err(|err| {
        ProviderError::fetch_failed(&format!(
            "Failed to read {} API response from {}: {}",
            spec.log_label, url, err
        ))
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct EndpointCandidate {
    pub(super) url: String,
    pub(super) allow_insecure_tls: bool,
}

pub(super) fn build_endpoint_candidates(
    port: u16,
    extension_port: Option<u16>,
) -> Vec<EndpointCandidate> {
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
    use crate::models::ProviderKind;

    fn antigravity_spec() -> CodeiumFamilySpec {
        CodeiumFamilySpec {
            kind: ProviderKind::Antigravity,
            provider_id: "antigravity:api",
            display_name: "Antigravity",
            brand_name: "Codeium",
            icon_asset: "src/icons/provider-antigravity.svg",
            dashboard_url: "",
            account_hint: "Codeium account",
            source_label: "local api",
            log_label: "Antigravity",
            ide_name: "antigravity",
            unavailable_message: "Antigravity live source and local cache are both unavailable",
            cache_db_config_relative_path: "Antigravity/User/globalStorage/state.vscdb",
            auth_status_key_candidates: &["antigravityAuthStatus"],
            process_markers: &[
                "--app_data_dir antigravity",
                "/antigravity/",
                ".antigravity/",
                "/antigravity.app/",
            ],
            cached_plan_info_key_candidates: &[],
        }
    }

    #[test]
    fn test_parse_process_line_success() {
        let line =
            "12345 language_server_macos_arm --csrf_token abc123 --extension_server_port 4242";
        let process = parse_process_line(line).unwrap();

        assert_eq!(process.pid, "12345");
        assert_eq!(
            process.binary_path,
            Some("language_server_macos_arm".to_string())
        );
        assert_eq!(process.csrf_token, Some("abc123".to_string()));
        assert_eq!(process.extension_port, Some(4242));
    }

    #[test]
    fn test_parse_process_line_without_csrf_token() {
        // 新版 Windsurf 不再通过命令行传递 csrf_token
        let line = "11162 /Applications/Windsurf.app/Contents/Resources/app/extensions/windsurf/bin/language_server_macos_arm --api_server_url https://server.codeium.com --run_child --enable_lsp --extension_server_port 59012 --ide_name windsurf --random_port --stdin_initial_metadata";
        let process = parse_process_line(line).unwrap();

        assert_eq!(process.pid, "11162");
        assert_eq!(
            process.binary_path,
            Some("/Applications/Windsurf.app/Contents/Resources/app/extensions/windsurf/bin/language_server_macos_arm".to_string())
        );
        assert_eq!(process.csrf_token, None);
        assert_eq!(process.extension_port, Some(59012));
    }

    #[test]
    fn test_parse_process_line_with_space_in_app_bundle_path() {
        let line = "22222 /Applications/Windsurf 2.app/Contents/Resources/app/extensions/windsurf/bin/language_server_macos_arm --api_server_url https://server.codeium.com --extension_server_port 59012 --ide_name windsurf";
        let process = parse_process_line(line).unwrap();

        assert_eq!(process.pid, "22222");
        assert_eq!(
            process.binary_path,
            Some(
                "/Applications/Windsurf 2.app/Contents/Resources/app/extensions/windsurf/bin/language_server_macos_arm"
                    .to_string()
            )
        );
        assert_eq!(process.extension_port, Some(59012));
    }

    #[test]
    fn test_matches_process_line_with_app_data_dir() {
        let line = "53319 /Applications/Antigravity.app/Contents/Resources/app/extensions/antigravity/bin/language_server_macos_arm --enable_lsp --csrf_token abc --extension_server_port 57048 --app_data_dir antigravity";
        assert!(matches_process_line(line, &antigravity_spec()));
    }

    #[test]
    fn test_matches_process_line_rejects_non_language_server() {
        let line = "99999 /usr/bin/some_other_process --app_data_dir antigravity";
        assert!(!matches_process_line(line, &antigravity_spec()));
    }

    #[test]
    fn test_matches_process_line_accepts_linux_language_server() {
        let line = "12345 /usr/share/antigravity/resources/app/extensions/antigravity/bin/language_server_linux_x64 --enable_lsp --app_data_dir antigravity";
        assert!(matches_process_line(line, &antigravity_spec()));
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
