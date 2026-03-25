use super::AiProvider;
use crate::models::{ProviderKind, QuotaInfo};
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

    /// 获取配置文件路径
    fn settings_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("BananaTray")
            .join("settings.json")
    }

    /// 从配置文件或环境变量读取 GitHub Token
    fn get_token(&self) -> Option<String> {
        // 优先从配置文件读取
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

        // 后备：从环境变量读取
        std::env::var("GITHUB_TOKEN").ok().filter(|t| !t.is_empty())
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

        // 使用 Copilot Internal API - 更简单，直接返回配额信息
        let output = Command::new("curl")
            .args([
                "-s",
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

        // 解析响应
        let resp: CopilotInternalResponse = serde_json::from_str(&output_str)
            .context("Failed to parse Copilot Internal API response.")?;

        let plan = resp.copilot_plan.unwrap_or_else(|| "unknown".to_string());

        // 检查是否有 premium_interactions 配额
        let quota = if let Some(snapshots) = resp.quota_snapshots {
            if let Some(interactions) = snapshots.premium_interactions {
                if interactions.unlimited.unwrap_or(false) {
                    QuotaInfo::new("Premium Requests (Unlimited)", 0.0, 0.0)
                } else {
                    let used = (interactions.entitlement - interactions.remaining) as f64;
                    let limit = interactions.entitlement as f64;
                    QuotaInfo::new(format!("Premium Requests ({})", plan), used, limit)
                }
            } else {
                // 没有 premium_interactions 配额（可能是只有基础 chat/completions）
                QuotaInfo::new("Premium Requests", 0.0, 0.0)
            }
        } else {
            bail!("No quota data found in Copilot API response");
        };

        Ok(vec![quota])
    }
}
