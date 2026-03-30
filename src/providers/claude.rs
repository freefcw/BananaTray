use super::{AiProvider, ProviderError};
use crate::models::{ProviderKind, ProviderMetadata, QuotaInfo, QuotaType};
use crate::utils::text_utils;
use anyhow::Result;
use async_trait::async_trait;
use log::debug;
use regex::Regex;
use std::process::Command;
use std::sync::LazyLock;

// 预编译的正则表达式（避免每次调用时重复编译）
static PCT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\d+)%\s+(left|used)").unwrap());
static RESET_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^Resets?\s+(.+)").unwrap());
static COST_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\$([0-9,]+\.?\d*)\s*/\s*\$([0-9,]+\.?\d*)").unwrap());
static MODEL_NAME_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\(([^)]+)\)").unwrap());

super::define_unit_provider!(ClaudeProvider);

impl ClaudeProvider {
    /// Read account email from ~/.claude.json if available.
    #[allow(dead_code)]
    pub fn read_account_email() -> Option<String> {
        let home = dirs::home_dir()?;
        let path = home.join(".claude.json");
        let content = std::fs::read_to_string(path).ok()?;
        let json: serde_json::Value = serde_json::from_str(&content).ok()?;
        json.get("oauthAccount")
            .and_then(|a| a.get("emailAddress"))
            .and_then(|e| e.as_str())
            .map(|s| s.to_string())
    }

    /// Parse the output of `claude /usage` into quota entries.
    fn parse_usage_output(raw: &str) -> Result<Vec<QuotaInfo>> {
        let clean = text_utils::strip_ansi(raw);

        // Split into sections by blank lines
        let sections = Self::split_sections(&clean);

        let mut quotas = Vec::new();

        for section in &sections {
            let lines: Vec<&str> = section.lines().map(|l| l.trim()).collect();
            if lines.is_empty() {
                continue;
            }

            // First non-empty line is the section label
            let header = lines[0];
            let header_lower = header.to_lowercase();

            // Determine quota type and display label
            let (quota_type, label) = if header_lower.contains("extra usage") {
                (QuotaType::Credit, "Extra Usage".to_string())
            } else if header_lower.contains("session") {
                (QuotaType::Session, "Session (5h)".to_string())
            } else if header_lower.contains("week") {
                if let Some(model) = Self::extract_model_name(header) {
                    (
                        QuotaType::ModelSpecific(model.clone()),
                        format!("Weekly ({})", model),
                    )
                } else {
                    (QuotaType::Weekly, "Weekly".to_string())
                }
            } else {
                continue;
            };

            let section_text = lines.join("\n");

            // Extract percentage
            let (used_pct, _percent_left) = if let Some(caps) = PCT_RE.captures(&section_text) {
                let value: f64 = caps[1].parse().unwrap_or(0.0);
                let direction = &caps[2];
                if direction == "used" {
                    (value, 100.0 - value)
                } else {
                    // "left"
                    (100.0 - value, value)
                }
            } else {
                continue;
            };

            // Extract reset time
            let reset_at = lines.iter().find_map(|line| {
                RESET_RE
                    .captures(line)
                    .map(|caps| caps[1].trim().to_string())
            });

            // For credit/extra usage, try to extract dollar amounts
            if quota_type == QuotaType::Credit {
                if let Some(caps) = COST_RE.captures(&section_text) {
                    let spent: f64 = caps[1].replace(',', "").parse().unwrap_or(0.0);
                    let budget: f64 = caps[2].replace(',', "").parse().unwrap_or(0.0);
                    quotas.push(QuotaInfo::with_details(
                        label, spent, budget, quota_type, reset_at,
                    ));
                    continue;
                }
            }

            // Percentage-based quota (used/limit as percentages out of 100)
            quotas.push(QuotaInfo::with_details(
                label, used_pct, 100.0, quota_type, reset_at,
            ));
        }

        Ok(quotas)
    }

    /// Split cleaned text into sections separated by blank lines,
    /// skipping the version header line.
    fn split_sections(text: &str) -> Vec<String> {
        let mut sections = Vec::new();
        let mut current = String::new();

        for line in text.lines() {
            let trimmed = line.trim();
            // Skip version header (e.g. "Claude Code v1.0.27")
            if trimmed.starts_with("Claude Code") {
                continue;
            }
            if trimmed.is_empty() {
                if !current.trim().is_empty() {
                    sections.push(current.clone());
                }
                current.clear();
            } else {
                if !current.is_empty() {
                    current.push('\n');
                }
                current.push_str(trimmed);
            }
        }
        if !current.trim().is_empty() {
            sections.push(current);
        }

        sections
    }

    /// Extract model name from headers like "Current week (Opus)".
    fn extract_model_name(header: &str) -> Option<String> {
        let caps = MODEL_NAME_RE.captures(header)?;
        let name = caps[1].trim().to_string();
        let lower = name.to_lowercase();
        // "all models" is the aggregate weekly, not a specific model
        if lower == "all models" {
            None
        } else {
            Some(name)
        }
    }
}

#[async_trait]
impl AiProvider for ClaudeProvider {
    fn metadata(&self) -> ProviderMetadata {
        ProviderMetadata {
            kind: ProviderKind::Claude,
            display_name: "Claude".into(),
            brand_name: "Anthropic".into(),
            icon_asset: "src/icons/provider-claude.svg".into(),
            dashboard_url: "https://console.anthropic.com/settings/billing".into(),
            account_hint: "Anthropic workspace".into(),
            source_label: "claude cli".into(),
        }
    }

    fn id(&self) -> &'static str {
        "claude:cli"
    }

    async fn is_available(&self) -> bool {
        Command::new("claude").arg("--version").output().is_ok()
    }

    async fn refresh(&self) -> Result<Vec<QuotaInfo>> {
        let output = Command::new("claude")
            .args(["/usage", "--allowed-tools", ""])
            .env_remove("CLAUDE_CODE_OAUTH_TOKEN")
            .output()
            .map_err(|_| ProviderError::cli_not_found("claude"))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{}\n{}", stdout, stderr);
        let combined_lower = combined.to_lowercase();

        debug!(target: "providers", "claude command exit code: {:?}", output.status.code());

        if !output.status.success() && stdout.trim().is_empty() {
            if combined_lower.contains("not logged in") || combined_lower.contains("authentication")
            {
                return Err(ProviderError::auth_required(Some("请运行 `claude` 登录")).into());
            }
            if combined_lower.contains("update") {
                return Err(ProviderError::update_required(None).into());
            }
            return Err(ProviderError::fetch_failed(&format!(
                "命令失败 (exit {:?})",
                output.status.code()
            ))
            .into());
        }

        debug!(target: "providers", "parsing claude usage output ({} bytes)", stdout.len());
        let quotas = Self::parse_usage_output(&stdout)?;

        if quotas.is_empty() {
            // Try to detect specific issues from the output
            if combined_lower.contains("not logged in") || combined_lower.contains("authentication")
            {
                return Err(ProviderError::auth_required(Some("请运行 `claude` 登录")).into());
            }
            if combined_lower.contains("update") {
                return Err(ProviderError::update_required(None).into());
            }
            return Err(ProviderError::parse_failed(&format!(
                "无法解析配额数据，原始输出:\n{}",
                stdout.trim()
            ))
            .into());
        }

        Ok(quotas)
    }
}
