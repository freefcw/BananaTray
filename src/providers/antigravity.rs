use super::{AiProvider, ProviderError};
use crate::models::{ProviderKind, ProviderMetadata, QuotaInfo, QuotaType, RefreshData};
use crate::utils::time_utils;
use anyhow::{Context, Result};
use async_trait::async_trait;
use log::{debug, info, warn};
use regex::Regex;
use std::process::Command;
use std::sync::LazyLock;
use ureq::Agent;

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

    /// Step 4: Parse the GetUserStatus JSON response
    fn parse_user_status(body: &str) -> Result<(Vec<QuotaInfo>, Option<String>, Option<String>)> {
        let json: serde_json::Value = serde_json::from_str(body)
            .map_err(|_| ProviderError::parse_failed("Antigravity API response"))?;

        let user_status = json
            .get("userStatus")
            .ok_or_else(|| ProviderError::parse_failed("missing 'userStatus' field"))?;

        // Extract email
        let email = user_status
            .get("email")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        // Extract plan name
        // 优先使用 userTier.name（能正确区分 Pro/Ultra/Free），
        // planInfo.planName 对 Pro 和 Ultra 都返回 "Pro"，不可靠
        let plan_name = user_status
            .pointer("/userTier/name")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .or_else(|| {
                user_status
                    .pointer("/planStatus/planInfo/planName")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
            })
            .map(|s| s.to_string());

        // Extract per-model quotas from cascadeModelConfigData
        let mut quotas = Vec::new();

        let model_configs = user_status
            .pointer("/cascadeModelConfigData/clientModelConfigs")
            .and_then(|v| v.as_array());

        if let Some(configs) = model_configs {
            for config in configs {
                let label = config
                    .get("label")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");

                if let Some(quota_info) = config.get("quotaInfo") {
                    // protobuf 默认值省略：remainingFraction 缺失时视为 0.0（额度耗尽）
                    let fraction = quota_info
                        .get("remainingFraction")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);

                    let used_percent = (1.0 - fraction) * 100.0;

                    let reset_text = quota_info
                        .get("resetTime")
                        .and_then(|v| v.as_str())
                        .and_then(time_utils::format_reset_countdown);

                    quotas.push(QuotaInfo::with_details(
                        label,
                        used_percent,
                        100.0,
                        QuotaType::ModelSpecific(label.to_string()),
                        reset_text,
                    ));
                }
            }
        }

        // Sort by label for consistent display
        quotas.sort_by(|a, b| a.label.cmp(&b.label));

        if quotas.is_empty() {
            return Err(ProviderError::no_data().into());
        }

        Ok((quotas, email, plan_name))
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

        let body = Self::fetch_user_status(port, &process.csrf_token, process.extension_port)?;

        let (quotas, email, plan_name) = Self::parse_user_status(&body)?;

        Ok(RefreshData::with_account(quotas, email, plan_name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- parse_user_status ---

    #[test]
    fn test_parse_full_response() {
        let json = r#"{
            "userStatus": {
                "email": "user@example.com",
                "userTier": { "id": "g1-ultra-tier", "name": "Google AI Ultra" },
                "planStatus": { "planInfo": { "planName": "Pro" } },
                "cascadeModelConfigData": {
                    "clientModelConfigs": [
                        {
                            "label": "claude-3-5-sonnet",
                            "quotaInfo": {
                                "remainingFraction": 0.75,
                                "resetTime": "2099-01-01T00:00:00Z"
                            }
                        },
                        {
                            "label": "gpt-4o",
                            "quotaInfo": {
                                "remainingFraction": 0.5
                            }
                        }
                    ]
                }
            }
        }"#;

        let (quotas, email, plan) = AntigravityProvider::parse_user_status(json).unwrap();

        assert_eq!(email, Some("user@example.com".to_string()));
        // userTier.name 优先于 planInfo.planName
        assert_eq!(plan, Some("Google AI Ultra".to_string()));
        assert_eq!(quotas.len(), 2);

        // Sorted by label
        assert_eq!(quotas[0].label, "claude-3-5-sonnet");
        assert!((quotas[0].used - 25.0).abs() < 0.01); // 1.0 - 0.75 = 0.25 => 25%
        assert_eq!(quotas[0].limit, 100.0);

        assert_eq!(quotas[1].label, "gpt-4o");
        assert!((quotas[1].used - 50.0).abs() < 0.01); // 1.0 - 0.5 = 0.5 => 50%
    }

    #[test]
    fn test_parse_user_tier_priority() {
        // userTier.name 应优先于 planInfo.planName
        let json = r#"{
            "userStatus": {
                "userTier": { "name": "Google AI Ultra" },
                "planStatus": { "planInfo": { "planName": "Pro" } },
                "cascadeModelConfigData": {
                    "clientModelConfigs": [
                        { "label": "m", "quotaInfo": { "remainingFraction": 0.5 } }
                    ]
                }
            }
        }"#;
        let (_, _, plan) = AntigravityProvider::parse_user_status(json).unwrap();
        assert_eq!(plan, Some("Google AI Ultra".to_string()));
    }

    #[test]
    fn test_parse_fallback_to_plan_name() {
        // 没有 userTier 时，回退到 planInfo.planName
        let json = r#"{
            "userStatus": {
                "planStatus": { "planInfo": { "planName": "Pro" } },
                "cascadeModelConfigData": {
                    "clientModelConfigs": [
                        { "label": "m", "quotaInfo": { "remainingFraction": 0.5 } }
                    ]
                }
            }
        }"#;
        let (_, _, plan) = AntigravityProvider::parse_user_status(json).unwrap();
        assert_eq!(plan, Some("Pro".to_string()));
    }

    #[test]
    fn test_parse_empty_user_tier_falls_back() {
        // userTier.name 为空字符串时，应回退到 planInfo.planName
        let json = r#"{
            "userStatus": {
                "userTier": { "name": "" },
                "planStatus": { "planInfo": { "planName": "Pro" } },
                "cascadeModelConfigData": {
                    "clientModelConfigs": [
                        { "label": "m", "quotaInfo": { "remainingFraction": 0.5 } }
                    ]
                }
            }
        }"#;
        let (_, _, plan) = AntigravityProvider::parse_user_status(json).unwrap();
        assert_eq!(plan, Some("Pro".to_string()));
    }

    #[test]
    fn test_parse_no_quotas() {
        let json = r#"{
            "userStatus": {
                "email": "user@example.com",
                "planStatus": { "planInfo": { "planName": "Free" } },
                "cascadeModelConfigData": {
                    "clientModelConfigs": []
                }
            }
        }"#;

        let result = AntigravityProvider::parse_user_status(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_missing_user_status() {
        let json = r#"{}"#;
        let result = AntigravityProvider::parse_user_status(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_no_email() {
        let json = r#"{
            "userStatus": {
                "cascadeModelConfigData": {
                    "clientModelConfigs": [
                        {
                            "label": "model-a",
                            "quotaInfo": { "remainingFraction": 1.0 }
                        }
                    ]
                }
            }
        }"#;

        let (quotas, email, plan) = AntigravityProvider::parse_user_status(json).unwrap();
        assert_eq!(email, None);
        assert_eq!(plan, None);
        assert_eq!(quotas.len(), 1);
        assert!((quotas[0].used - 0.0).abs() < 0.01); // 100% remaining = 0% used
    }

    #[test]
    fn test_parse_model_without_quota_info() {
        let json = r#"{
            "userStatus": {
                "email": "test@test.com",
                "cascadeModelConfigData": {
                    "clientModelConfigs": [
                        { "label": "no-quota-model" },
                        {
                            "label": "has-quota",
                            "quotaInfo": { "remainingFraction": 0.3 }
                        }
                    ]
                }
            }
        }"#;

        let (quotas, _, _) = AntigravityProvider::parse_user_status(json).unwrap();
        // Only the model with quotaInfo is included
        assert_eq!(quotas.len(), 1);
        assert_eq!(quotas[0].label, "has-quota");
        assert!((quotas[0].used - 70.0).abs() < 0.01); // 1.0 - 0.3 = 0.7 => 70%
    }

    #[test]
    fn test_parse_quota_type_is_model_specific() {
        let json = r#"{
            "userStatus": {
                "cascadeModelConfigData": {
                    "clientModelConfigs": [
                        {
                            "label": "claude-3-5-sonnet",
                            "quotaInfo": { "remainingFraction": 0.5 }
                        }
                    ]
                }
            }
        }"#;

        let (quotas, _, _) = AntigravityProvider::parse_user_status(json).unwrap();
        assert_eq!(quotas.len(), 1);
        assert!(matches!(
            &quotas[0].quota_type,
            QuotaType::ModelSpecific(name) if name == "claude-3-5-sonnet"
        ));
    }

    #[test]
    fn test_parse_depleted_quota() {
        let json = r#"{
            "userStatus": {
                "cascadeModelConfigData": {
                    "clientModelConfigs": [
                        {
                            "label": "depleted-model",
                            "quotaInfo": { "remainingFraction": 0.0 }
                        }
                    ]
                }
            }
        }"#;

        let (quotas, _, _) = AntigravityProvider::parse_user_status(json).unwrap();
        assert_eq!(quotas.len(), 1);
        assert!((quotas[0].used - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_parse_missing_remaining_fraction_treated_as_depleted() {
        // protobuf 默认值省略：remainingFraction 为 0 时字段不序列化
        let json = r#"{
            "userStatus": {
                "cascadeModelConfigData": {
                    "clientModelConfigs": [
                        {
                            "label": "claude-opus",
                            "quotaInfo": { "resetTime": "2099-01-01T00:00:00Z" }
                        },
                        {
                            "label": "gemini-pro",
                            "quotaInfo": { "remainingFraction": 0.8, "resetTime": "2099-01-01T00:00:00Z" }
                        }
                    ]
                }
            }
        }"#;

        let (quotas, _, _) = AntigravityProvider::parse_user_status(json).unwrap();
        assert_eq!(quotas.len(), 2);

        // Sorted by label: claude-opus, gemini-pro
        assert_eq!(quotas[0].label, "claude-opus");
        assert!((quotas[0].used - 100.0).abs() < 0.01); // 缺失 = 0.0 剩余 = 100% 已用

        assert_eq!(quotas[1].label, "gemini-pro");
        assert!((quotas[1].used - 20.0).abs() < 0.01); // 0.8 剩余 = 20% 已用
    }

    // --- regex tests ---

    #[test]
    fn test_csrf_regex() {
        let line = "12345 /path/to/language_server_macos_arm --csrf_token abc123def --other_flag";
        let caps = CSRF_RE.captures(line).unwrap();
        assert_eq!(caps.get(1).unwrap().as_str(), "abc123def");
    }

    #[test]
    fn test_ext_port_regex() {
        let line =
            "12345 /path/to/language_server_macos --extension_server_port 42100 --csrf_token xyz";
        let caps = EXT_PORT_RE.captures(line).unwrap();
        assert_eq!(caps.get(1).unwrap().as_str(), "42100");
    }

    #[test]
    fn test_listen_port_regex() {
        let line = "node    12345 user   20u  IPv4 0x1234  0t0  TCP 127.0.0.1:65432 (LISTEN)";
        let caps = LISTEN_PORT_RE.captures(line).unwrap();
        assert_eq!(caps.get(1).unwrap().as_str(), "65432");
    }

    #[test]
    fn test_csrf_regex_no_match() {
        let line = "12345 /path/to/other_process --no-csrf";
        assert!(CSRF_RE.captures(line).is_none());
    }
}
