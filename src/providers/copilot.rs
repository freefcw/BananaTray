use super::AiProvider;
use crate::models::{ProviderKind, QuotaInfo, QuotaType};
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use std::path::PathBuf;
use std::process::Command;

pub struct CopilotProvider {}

impl CopilotProvider {
    pub fn new() -> Self {
        Self {}
    }

    /// 获取配置文件路径（与 settings_store 保持一致）
    fn settings_path() -> PathBuf {
        crate::settings_store::config_path()
    }

    /// 从配置文件、GitHub Copilot 扩展配置或环境变量读取 GitHub Token
    fn get_token(&self) -> Option<String> {
        // 1. 优先从 BananaTray 配置文件读取
        if let Ok(content) = std::fs::read_to_string(Self::settings_path()) {
            if let Ok(settings) = serde_json::from_str::<serde_json::Value>(&content) {
                let token = settings
                    .get("providers")
                    .and_then(|p| p.get("github_token"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                if let Some(t) = token {
                    if !t.is_empty() {
                        return Some(t);
                    }
                }
            }
        }

        // 2. 从 GitHub Copilot 扩展配置读取 OAuth token
        if let Some(token) = Self::read_copilot_oauth_token() {
            return Some(token);
        }

        // 3. 后备：从环境变量读取
        std::env::var("GITHUB_TOKEN").ok().filter(|t| !t.is_empty())
    }

    /// 从 ~/.config/github-copilot/ 读取已有的 OAuth token
    fn read_copilot_oauth_token() -> Option<String> {
        let home = dirs::home_dir()?;
        let copilot_dir = home.join(".config").join("github-copilot");

        // 尝试 hosts.json（旧版）和 apps.json（新版）
        for filename in &["hosts.json", "apps.json"] {
            let path = copilot_dir.join(filename);
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    // 格式: { "github.com": { "oauth_token": "gho_xxx", ... } }
                    // 或: { "github.com:copilot": { "oauth_token": "gho_xxx", ... } }
                    if let Some(obj) = json.as_object() {
                        for (key, value) in obj {
                            if key.contains("github.com") {
                                if let Some(token) = value
                                    .get("oauth_token")
                                    .and_then(|v| v.as_str())
                                    .filter(|s| !s.is_empty())
                                {
                                    return Some(token.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    }
}

/// Copilot Internal API 响应结构
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct CopilotInternalResponse {
    copilot_plan: Option<String>,
    quota_snapshots: Option<QuotaSnapshots>,
}

#[derive(Debug, Deserialize)]
struct QuotaSnapshots {
    premium_interactions: Option<InteractionQuota>,
}

#[derive(Debug, Deserialize)]
struct InteractionQuota {
    entitlement: i32,
    remaining: i32,
    #[allow(dead_code)]
    percent_remaining: f64,
    unlimited: Option<bool>,
}

#[async_trait]
impl AiProvider for CopilotProvider {
    fn id(&self) -> &'static str {
        "copilot:api"
    }

    fn kind(&self) -> ProviderKind {
        ProviderKind::Copilot
    }

    async fn is_available(&self) -> bool {
        self.get_token().is_some()
    }

    async fn refresh(&self) -> Result<Vec<QuotaInfo>> {
        let token = self
            .get_token()
            .context("GitHub token not configured. Set github_token in settings, or GITHUB_TOKEN environment variable.")?;

        let output = Command::new("curl")
            .args([
                "-s",
                "-w",
                "\n%{http_code}",
                "-H",
                &format!("Authorization: Bearer {}", token),
                "-H",
                "Accept: application/json",
                "https://api.github.com/copilot_internal/user",
            ])
            .output()
            .context("Error launching curl command to reach GitHub Copilot Internal API.")?;

        if !output.status.success() {
            bail!("curl to GitHub API unexpectedly failed.");
        }

        let output_str = String::from_utf8_lossy(&output.stdout);

        // Extract HTTP status code from last line
        let (body, status_code) = {
            let trimmed = output_str.trim();
            if let Some(pos) = trimmed.rfind('\n') {
                let code = trimmed[pos + 1..].trim();
                let body = &trimmed[..pos];
                (body.to_string(), code.to_string())
            } else {
                (trimmed.to_string(), String::new())
            }
        };

        match status_code.as_str() {
            "401" => bail!(
                "GitHub token is invalid or expired. Update your token in Settings → Providers."
            ),
            "403" => {
                bail!("Token lacks required permissions. Use a Classic PAT with 'copilot' scope.")
            }
            "404" => bail!(
                "Copilot not enabled for this account. Check your GitHub Copilot subscription."
            ),
            _ => {}
        }

        // 解析响应
        let resp: CopilotInternalResponse = serde_json::from_str(&body)
            .context("Failed to parse Copilot Internal API response.")?;

        let plan = resp.copilot_plan.unwrap_or_else(|| "unknown".to_string());
        let plan_label = capitalize_first(&plan);

        // 检查是否有 premium_interactions 配额
        let quota = if let Some(snapshots) = resp.quota_snapshots {
            if let Some(interactions) = snapshots.premium_interactions {
                if interactions.unlimited.unwrap_or(false) {
                    QuotaInfo::with_details(
                        format!("Premium Requests ({})", plan_label),
                        0.0,
                        0.0,
                        QuotaType::General,
                        Some("Unlimited".to_string()),
                    )
                } else {
                    let used = (interactions.entitlement - interactions.remaining).max(0) as f64;
                    let limit = interactions.entitlement as f64;
                    QuotaInfo::with_details(
                        format!("Premium Requests ({})", plan_label),
                        used,
                        limit,
                        QuotaType::Weekly,
                        None,
                    )
                }
            } else {
                QuotaInfo::with_details(
                    format!("Chat & Completions ({})", plan_label),
                    0.0,
                    0.0,
                    QuotaType::General,
                    Some("Unlimited".to_string()),
                )
            }
        } else {
            bail!("No quota data found in Copilot API response.");
        };

        Ok(vec![quota])
    }
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}
