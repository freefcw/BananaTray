//! Codex CLI RPC fallback：通过 `codex app-server` 读取结构化 rate limit。
//!
//! 这条路径位于 OAuth HTTP 之后、PTY `/status` 之前。相比 PTY 文本解析，
//! app-server 返回 JSON-RPC 结构化字段（usedPercent / resetsAt / credits / planType），
//! 不依赖 TUI 布局、ANSI 或输出文案；但它仍属于 Codex CLI 的 experimental 接口，
//! 因此失败时继续保留 PTY `/status` 作为最后兜底。

use crate::models::{QuotaDetailSpec, QuotaInfo, QuotaLabelSpec, QuotaType};
use crate::providers::common::path_resolver;
use crate::providers::{ProviderError, ProviderResult};
use anyhow::Result;
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use super::parser::{resolve_role_from_minutes, ParsedUsage, WindowRole};

const CODEX_BINARY: &str = "codex";
const RPC_TIMEOUT: Duration = Duration::from_secs(8);

pub(super) fn fetch_via_rpc() -> ProviderResult<ParsedUsage> {
    let executable = path_resolver::locate_executable(CODEX_BINARY)
        .ok_or_else(|| ProviderError::cli_not_found(CODEX_BINARY))?;
    let mut client = RpcClient::spawn(&executable)?;
    client.initialize()?;
    let rate_limits = client.fetch_rate_limits()?;
    parse_rate_limits(rate_limits)
}

struct RpcClient {
    child: Child,
    stdin: ChildStdin,
    rx: mpsc::Receiver<Result<Value, String>>,
    next_id: u64,
}

impl RpcClient {
    fn spawn(executable: &str) -> ProviderResult<Self> {
        let mut child = Command::new(executable)
            .args(["-s", "read-only", "-a", "untrusted", "app-server"])
            .env("PATH", path_resolver::enriched_path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| ProviderError::fetch_failed(&format!("codex app-server: {err}")))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| ProviderError::fetch_failed("codex app-server stdin unavailable"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| ProviderError::fetch_failed("codex app-server stdout unavailable"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| ProviderError::fetch_failed("codex app-server stderr unavailable"))?;

        // app-server 以 newline-delimited JSON-RPC 通信。单独线程阻塞读 stdout，
        // 主线程用 recv_timeout 控制每个请求的最大等待时间。
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let line = match line {
                    Ok(line) => line,
                    Err(err) => {
                        let _ = tx.send(Err(format!("read stdout failed: {err}")));
                        break;
                    }
                };
                if line.trim().is_empty() {
                    continue;
                }
                let parsed = serde_json::from_str::<Value>(&line)
                    .map_err(|err| format!("invalid JSON-RPC line: {err}"));
                if tx.send(parsed).is_err() {
                    break;
                }
            }
        });
        // stderr 只用于 Codex CLI 自身诊断；必须 drain，避免子进程日志写满管道后
        // 阻塞 app-server，导致主 JSON-RPC stdout 永远等不到响应。
        thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                log::debug!(target: "providers", "Codex app-server stderr: {line}");
            }
        });

        Ok(Self {
            child,
            stdin,
            rx,
            next_id: 1,
        })
    }

    fn initialize(&mut self) -> ProviderResult<()> {
        let id = self.next_request_id();
        self.send_request(
            id,
            "initialize",
            json!({
                "clientInfo": {
                    "name": "bananatray",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        )?;
        let _ = self.read_response(id)?;
        self.send_notification("initialized", json!({}))
    }

    fn fetch_rate_limits(&mut self) -> ProviderResult<Value> {
        let id = self.next_request_id();
        self.send_request(id, "account/rateLimits/read", json!({}))?;
        self.read_response(id)
    }

    fn next_request_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn send_request(&mut self, id: u64, method: &str, params: Value) -> ProviderResult<()> {
        self.write_payload(json!({
            "id": id,
            "method": method,
            "params": params,
        }))
    }

    fn send_notification(&mut self, method: &str, params: Value) -> ProviderResult<()> {
        self.write_payload(json!({
            "method": method,
            "params": params,
        }))
    }

    fn write_payload(&mut self, payload: Value) -> ProviderResult<()> {
        serde_json::to_writer(&mut self.stdin, &payload)
            .map_err(|err| ProviderError::fetch_failed(&format!("encode JSON-RPC: {err}")))?;
        self.stdin
            .write_all(b"\n")
            .map_err(|err| ProviderError::fetch_failed(&format!("write JSON-RPC: {err}")))?;
        self.stdin
            .flush()
            .map_err(|err| ProviderError::fetch_failed(&format!("flush JSON-RPC: {err}")))
    }

    fn read_response(&mut self, id: u64) -> ProviderResult<Value> {
        loop {
            let message = self.recv_message()?;
            if message.get("id").and_then(json_id) != Some(id) {
                // app-server 可能先发 notification；它们没有 id，直接跳过。
                continue;
            }

            if let Some(error) = message.get("error") {
                let detail = error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown JSON-RPC error");
                return Err(ProviderError::fetch_failed(&format!(
                    "codex app-server RPC error: {detail}"
                )));
            }

            return message
                .get("result")
                .cloned()
                .ok_or_else(|| ProviderError::parse_failed("codex app-server response"));
        }
    }

    fn recv_message(&mut self) -> ProviderResult<Value> {
        match self.rx.recv_timeout(RPC_TIMEOUT) {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(err)) => Err(ProviderError::parse_failed(&format!(
                "codex app-server output: {err}"
            ))),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let _ = self.child.kill();
                Err(ProviderError::Timeout)
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(ProviderError::fetch_failed(
                "codex app-server closed stdout before response",
            )),
        }
    }
}

impl Drop for RpcClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn json_id(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_i64().and_then(|n| u64::try_from(n).ok()))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetAccountRateLimitsResponse {
    rate_limits: RateLimitSnapshot,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RateLimitSnapshot {
    credits: Option<CreditsSnapshot>,
    plan_type: Option<String>,
    primary: Option<RateLimitWindow>,
    secondary: Option<RateLimitWindow>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreditsSnapshot {
    balance: Option<String>,
    has_credits: bool,
    unlimited: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RateLimitWindow {
    resets_at: Option<i64>,
    used_percent: f64,
    window_duration_mins: Option<i64>,
}

fn parse_rate_limits(value: Value) -> ProviderResult<ParsedUsage> {
    let response: GetAccountRateLimitsResponse = serde_json::from_value(value)
        .map_err(|_| ProviderError::parse_failed("codex app-server rate limits"))?;

    let mut quotas = Vec::new();
    if let Some(primary) = response.rate_limits.primary {
        quotas.push(build_window_quota(WindowRole::Session, primary));
    }
    if let Some(secondary) = response.rate_limits.secondary {
        quotas.push(build_window_quota(WindowRole::Weekly, secondary));
    }

    // 与 HTTP JSON 路径保持一致：服务端异常返回两个相同角色时，只保留第一个。
    if quotas.len() == 2 && quotas[0].quota_type == quotas[1].quota_type {
        quotas.truncate(1);
    }
    if let Some(credits) = response.rate_limits.credits {
        if let Some(balance) = read_credits_balance(&credits) {
            quotas.push(QuotaInfo::balance_only(
                QuotaLabelSpec::Credits,
                balance,
                None,
                QuotaType::Credit,
                None,
            ));
        }
    }

    if quotas.is_empty() {
        return Err(ProviderError::no_data());
    }

    Ok(ParsedUsage {
        quotas,
        plan_type: response
            .rate_limits
            .plan_type
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
    })
}

fn build_window_quota(default_role: WindowRole, window: RateLimitWindow) -> QuotaInfo {
    let role = resolve_role_from_minutes(window.window_duration_mins, default_role);

    QuotaInfo::with_details(
        role.label_spec(),
        window.used_percent,
        100.0,
        role.quota_type(),
        window
            .resets_at
            .map(|epoch_secs| QuotaDetailSpec::ResetAt { epoch_secs }),
    )
}

fn read_credits_balance(credits: &CreditsSnapshot) -> Option<f64> {
    if !credits.has_credits || credits.unlimited {
        return None;
    }
    credits
        .balance
        .as_deref()
        .and_then(|balance| balance.parse::<f64>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_rate_limits_maps_windows_credits_and_plan() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let raw = json!({
            "rateLimits": {
                "planType": "pro",
                "primary": {
                    "usedPercent": 30,
                    "resetsAt": 1735000000,
                    "windowDurationMins": 300
                },
                "secondary": {
                    "usedPercent": 50,
                    "resetsAt": 1735500000,
                    "windowDurationMins": 10080
                },
                "credits": {
                    "balance": "12.5",
                    "hasCredits": true,
                    "unlimited": false
                }
            }
        });

        let parsed = parse_rate_limits(raw).unwrap();

        assert_eq!(parsed.plan_type.as_deref(), Some("pro"));
        assert_eq!(parsed.quotas.len(), 3);
        assert_eq!(parsed.quotas[0].label_spec, QuotaLabelSpec::Session);
        assert_eq!(parsed.quotas[0].used, 30.0);
        assert!(matches!(
            parsed.quotas[0].detail_spec,
            Some(QuotaDetailSpec::ResetAt {
                epoch_secs: 1735000000
            })
        ));
        assert_eq!(parsed.quotas[1].label_spec, QuotaLabelSpec::Weekly);
        assert_eq!(parsed.quotas[1].used, 50.0);
        assert_eq!(parsed.quotas[2].label_spec, QuotaLabelSpec::Credits);
        assert_eq!(parsed.quotas[2].remaining_balance, Some(12.5));
    }

    #[test]
    fn parse_rate_limits_uses_window_duration_for_primary_weekly() {
        let raw = json!({
            "rateLimits": {
                "planType": "free",
                "primary": {
                    "usedPercent": 72,
                    "resetsAt": 1735500000,
                    "windowDurationMins": 10080
                }
            }
        });

        let parsed = parse_rate_limits(raw).unwrap();

        assert_eq!(parsed.plan_type.as_deref(), Some("free"));
        assert_eq!(parsed.quotas.len(), 1);
        assert_eq!(parsed.quotas[0].label_spec, QuotaLabelSpec::Weekly);
        assert_eq!(parsed.quotas[0].quota_type, QuotaType::Weekly);
        assert_eq!(parsed.quotas[0].used, 72.0);
    }

    #[test]
    fn parse_rate_limits_skips_unlimited_credits() {
        let raw = json!({
            "rateLimits": {
                "planType": "team",
                "primary": { "usedPercent": 10 },
                "credits": {
                    "balance": "999",
                    "hasCredits": true,
                    "unlimited": true
                }
            }
        });

        let parsed = parse_rate_limits(raw).unwrap();

        assert_eq!(parsed.quotas.len(), 1);
        assert_eq!(parsed.quotas[0].label_spec, QuotaLabelSpec::Session);
    }

    #[test]
    fn parse_rate_limits_returns_no_data_when_all_sources_empty() {
        let raw = json!({
            "rateLimits": {
                "planType": "free",
                "credits": {
                    "balance": null,
                    "hasCredits": false,
                    "unlimited": false
                }
            }
        });

        assert!(matches!(parse_rate_limits(raw), Err(ProviderError::NoData)));
    }

    #[test]
    fn json_id_accepts_unsigned_and_non_negative_signed_ids() {
        assert_eq!(json_id(&json!(2)), Some(2));
        assert_eq!(json_id(&json!(-1)), None);
        assert_eq!(json_id(&json!("2")), None);
    }
}
