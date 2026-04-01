mod parse_strategy;

use super::{AiProvider, ProviderError};
use crate::models::{ProviderKind, ProviderMetadata, RefreshData};
use anyhow::{Context, Result};
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD, Engine};
use log::{debug, info, warn};
use regex::Regex;
use std::process::Command;
use std::sync::LazyLock;
use ureq::Agent;

use parse_strategy::{ApiParseStrategy, CacheParseStrategy, ParseStrategy};

super::define_unit_provider!(AntigravityProvider);

/// Process names to search for via pgrep (Intel and ARM variants)
const PROCESS_NAMES: &[&str] = &["language_server_macos_arm", "language_server_macos"];

/// API path for GetUserStatus
const API_PATH: &str = "exa.language_server_pb.LanguageServerService/GetUserStatus";

static CSRF_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"--csrf_token\s+(\S+)").unwrap());
static EXT_PORT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"--extension_server_port\s+(\d+)").unwrap());
static LISTEN_PORT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":(\d+)\s+\(LISTEN\)").unwrap());

/// A ureq Agent that skips TLS certificate verification (for localhost self-signed certs)
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

/// Info extracted from a running Antigravity language server process
#[derive(Debug)]
struct ProcessInfo {
    pid: String,
    csrf_token: String,
    extension_port: Option<u16>,
}

impl AntigravityProvider {
    /// Step 1: Find the Antigravity language server process via pgrep
    fn detect_process() -> Result<ProcessInfo> {
        let output = Command::new("/usr/bin/pgrep")
            .args(["-lf", "language_server_macos"])
            .output()
            .map_err(|_| ProviderError::unavailable("pgrep not available"))?;

        let stdout = String::from_utf8_lossy(&output.stdout);

        if stdout.trim().is_empty() {
            return Err(
                ProviderError::unavailable("Antigravity language server not running").into(),
            );
        }

        // Find the matching process line
        for line in stdout.lines() {
            let is_match = PROCESS_NAMES.iter().any(|name| line.contains(name));
            if !is_match {
                continue;
            }

            // Extract PID (first token)
            let pid = line
                .split_whitespace()
                .next()
                .ok_or_else(|| ProviderError::parse_failed("no PID in pgrep output"))?
                .to_string();

            // Extract CSRF token from --csrf_token arg
            let csrf_token = CSRF_RE
                .captures(line)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string())
                .ok_or_else(|| {
                    ProviderError::parse_failed("--csrf_token not found in process args")
                })?;

            // Extract extension server port (optional fallback)
            let extension_port = EXT_PORT_RE
                .captures(line)
                .and_then(|c| c.get(1))
                .and_then(|m| m.as_str().parse::<u16>().ok());

            debug!(
                target: "providers",
                "Antigravity process found: pid={}, extension_port={:?}",
                pid, extension_port
            );

            return Ok(ProcessInfo {
                pid,
                csrf_token,
                extension_port,
            });
        }

        Err(ProviderError::unavailable("Antigravity language server not running").into())
    }

    /// Step 2: Discover the TCP listen port via lsof
    fn discover_port(pid: &str) -> Result<u16> {
        let output = Command::new("/usr/sbin/lsof")
            .args(["-nP", "-iTCP", "-sTCP:LISTEN", "-a", "-p", pid])
            .output()
            .map_err(|_| ProviderError::unavailable("lsof not available"))?;

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Find the listen port (may be multiple lines, pick the first)
        for cap in LISTEN_PORT_RE.captures_iter(&stdout) {
            if let Some(port_str) = cap.get(1) {
                if let Ok(port) = port_str.as_str().parse::<u16>() {
                    debug!(target: "providers", "Antigravity listen port: {}", port);
                    return Ok(port);
                }
            }
        }

        Err(ProviderError::parse_failed("no TCP LISTEN port found via lsof for Antigravity").into())
    }

    /// Step 3: Call the HTTP API to get user status
    fn fetch_user_status(
        port: u16,
        csrf_token: &str,
        extension_port: Option<u16>,
    ) -> Result<String> {
        let body = r#"{"metadata":{"ideName":"antigravity"}}"#;

        // Try HTTPS first (self-signed cert on localhost)
        let https_url = format!("https://127.0.0.1:{}/{}", port, API_PATH);
        match Self::post_api(&https_url, csrf_token, body, true) {
            Ok(resp) => return Ok(resp),
            Err(e) => {
                debug!(
                    target: "providers",
                    "Antigravity HTTPS failed (port {}): {}, trying fallback",
                    port, e
                );
            }
        }

        // Fallback to HTTP on extension port
        if let Some(ext_port) = extension_port {
            let http_url = format!("http://127.0.0.1:{}/{}", ext_port, API_PATH);
            match Self::post_api(&http_url, csrf_token, body, false) {
                Ok(resp) => return Ok(resp),
                Err(e) => {
                    debug!(
                        target: "providers",
                        "Antigravity HTTP fallback failed (port {}): {}",
                        ext_port, e
                    );
                }
            }
        }

        // Last resort: try HTTP on the main port
        let http_url = format!("http://127.0.0.1:{}/{}", port, API_PATH);
        Self::post_api(&http_url, csrf_token, body, false)
    }

    /// Send a POST request to the Antigravity API
    fn post_api(url: &str, csrf_token: &str, body: &str, use_insecure: bool) -> Result<String> {
        debug!(target: "providers", "Antigravity POST {}", url);

        let agent = if use_insecure {
            &*INSECURE_AGENT
        } else {
            // Plain HTTP agent (reuse from http_client would be ideal but we need
            // a separate one due to the insecure TLS config for HTTPS calls)
            static PLAIN_AGENT: LazyLock<Agent> = LazyLock::new(|| {
                Agent::new_with_config(
                    ureq::config::Config::builder()
                        .http_status_as_error(false)
                        .build(),
                )
            });
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

    /// Local cache fallback: Find Antigravity's state.vscdb database path
    fn find_local_cache_db() -> Result<std::path::PathBuf> {
        // macOS: ~/Library/Application Support/Antigravity/User/globalStorage/state.vscdb
        let home = dirs::home_dir()
            .ok_or_else(|| ProviderError::unavailable("cannot determine home directory"))?;

        let db_path = home
            .join("Library")
            .join("Application Support")
            .join("Antigravity")
            .join("User")
            .join("globalStorage")
            .join("state.vscdb");

        if !db_path.exists() {
            return Err(
                ProviderError::unavailable("Antigravity local cache database not found").into(),
            );
        }

        debug!(target: "providers", "Antigravity local cache DB: {}", db_path.display());
        Ok(db_path)
    }

    /// Read user status from local cache database
    fn read_local_cache() -> Result<RefreshData> {
        use rusqlite::{Connection, OpenFlags};

        let db_path = Self::find_local_cache_db()?;

        // Open database in read-only mode
        let conn = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .with_context(|| format!("cannot open Antigravity cache DB: {}", db_path.display()))?;

        // Query antigravityAuthStatus key from ItemTable
        let auth_status_json: String = conn
            .query_row(
                "SELECT value FROM ItemTable WHERE key = 'antigravityAuthStatus'",
                [],
                |row| row.get(0),
            )
            .map_err(|e| {
                ProviderError::parse_failed(&format!("cannot query antigravityAuthStatus: {}", e))
            })?;

        // Parse JSON to extract userStatusProtoBinaryBase64
        let auth_status: serde_json::Value =
            serde_json::from_str(&auth_status_json).map_err(|e| {
                ProviderError::parse_failed(&format!("invalid auth status JSON: {}", e))
            })?;

        let user_status_b64 = auth_status
            .get("userStatusProtoBinaryBase64")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ProviderError::parse_failed("missing userStatusProtoBinaryBase64 field")
            })?;

        // Decode base64
        let user_status_data = STANDARD.decode(user_status_b64).map_err(|e| {
            ProviderError::parse_failed(&format!("invalid user status base64: {}", e))
        })?;

        // Use cache parse strategy
        let strategy = CacheParseStrategy;
        let (quotas, email, plan_name) = strategy.parse(&user_status_data)?;

        info!(
            target: "providers",
            "Antigravity local cache: found {} model quotas for {}",
            quotas.len(),
            email.as_deref().unwrap_or("unknown user")
        );

        Ok(RefreshData::with_account(quotas, email, plan_name))
    }
}

#[async_trait]
impl AiProvider for AntigravityProvider {
    fn metadata(&self) -> ProviderMetadata {
        ProviderMetadata {
            kind: ProviderKind::Antigravity,
            display_name: "Antigravity".into(),
            brand_name: "Codeium".into(),
            icon_asset: "src/icons/provider-antigravity.svg".into(),
            dashboard_url: "https://codeium.com/account".into(),
            account_hint: "Codeium account".into(),
            source_label: "local api".into(),
        }
    }

    fn id(&self) -> &'static str {
        "antigravity:api"
    }

    async fn is_available(&self) -> bool {
        Self::detect_process().is_ok()
    }

    async fn refresh(&self) -> Result<RefreshData> {
        let process = Self::detect_process()?;

        let port = Self::discover_port(&process.pid).unwrap_or_else(|e| {
            warn!(
                target: "providers",
                "Antigravity lsof port discovery failed: {}, using extension_port fallback", e
            );
            process.extension_port.unwrap_or(0)
        });

        if port == 0 {
            return Err(ProviderError::unavailable("cannot determine Antigravity API port").into());
        }

        info!(
            target: "providers",
            "Antigravity: fetching user status from port {} (ext: {:?})",
            port, process.extension_port
        );

        // Try API first
        match Self::fetch_user_status(port, &process.csrf_token, process.extension_port) {
            Ok(body) => {
                // Use API parse strategy
                let strategy = ApiParseStrategy;
                match strategy.parse(body.as_bytes()) {
                    Ok((quotas, email, plan_name)) => {
                        Ok(RefreshData::with_account(quotas, email, plan_name))
                    }
                    Err(e) => {
                        warn!(target: "providers", "Antigravity API parse failed: {}, trying local cache", e);
                        Self::read_local_cache()
                    }
                }
            }
            Err(e) => {
                // API call failed (likely OAuth issue), try local cache fallback
                warn!(
                    target: "providers",
                    "Antigravity API failed: {}, falling back to local cache",
                    e
                );
                Self::read_local_cache()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Tests will be moved to parse_strategy module
}
