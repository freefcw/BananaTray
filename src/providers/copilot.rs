use super::AiProvider;
use crate::models::{ProviderKind, QuotaInfo};
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::Value;
use std::process::Command;

pub struct CopilotProvider {}

impl CopilotProvider {
    pub fn new() -> Self {
        Self {}
    }
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
        let username = std::env::var("GITHUB_USERNAME").unwrap_or_default();
        let token = std::env::var("GITHUB_TOKEN").unwrap_or_default();
        !username.is_empty() && !token.is_empty()
    }

    async fn refresh(&self) -> Result<Vec<QuotaInfo>> {
        let username = std::env::var("GITHUB_USERNAME")
            .context("Missing environment variable 'GITHUB_USERNAME'")?;
        let token =
            std::env::var("GITHUB_TOKEN").context("Missing environment variable 'GITHUB_TOKEN'")?;

        let url = format!(
            "https://api.github.com/users/{}/settings/billing/premium_request/usage",
            username
        );

        let output = Command::new("curl")
            .args([
                "-s", // silent
                "-H",
                &format!("Authorization: Bearer {}", token),
                "-H",
                "Accept: application/vnd.github+json",
                "-H",
                "X-GitHub-Api-Version: 2022-11-28",
                &url,
            ])
            .output()
            .context("Error launching curl Command to reach GitHub API.")?;

        if !output.status.success() {
            anyhow::bail!("Curl to GitHub API unexpectedly failed.");
        }

        let output_str = String::from_utf8_lossy(&output.stdout);
        let resp: Value =
            serde_json::from_str(&output_str).context("Invalid JSON returned from GitHub API.")?;

        if let Some(error_message) = resp.get("message") {
            anyhow::bail!("GitHub API Error: {}", error_message.as_str().unwrap_or(""));
        }

        let mut total_requests = 0.0;
        if let Some(items) = resp.get("usageItems").and_then(|v| v.as_array()) {
            for item in items {
                if let Some(product_name) = item.get("product").and_then(|v| v.as_str()) {
                    if product_name.to_lowercase().contains("copilot") {
                        let gross = item
                            .get("grossQuantity")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0);
                        total_requests += gross;
                    }
                }
            }
        } else {
            // Check if there are just no items or if it's completely malformed
            if !output_str.contains("usageItems") {
                anyhow::bail!(
                    "Unrecognized data format from Github API endpoints: {}",
                    output_str.chars().take(100).collect::<String>()
                );
            }
        }

        // 以默认每月限额 50（标准配置）做计算
        Ok(vec![QuotaInfo::new(
            "Monthly Requests",
            total_requests,
            50.0,
        )])
    }
}
