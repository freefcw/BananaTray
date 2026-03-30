use super::AiProvider;
use crate::models::{ProviderKind, ProviderMetadata, QuotaInfo};
use anyhow::{Context, Result};
use async_trait::async_trait;
use regex::Regex;
use std::process::Command;
use std::sync::LazyLock;

// 预编译的正则表达式
static CREDIT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(.+?):\s*\$([0-9]+(?:\.[0-9]+)?)\s*/\s*\$([0-9]+(?:\.[0-9]+)?)\s+remaining")
        .unwrap()
});
static BALANCE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(.+?):\s*\$([0-9]+(?:\.[0-9]+)?)\s+remaining").unwrap());

super::define_unit_provider!(AmpProvider);

#[async_trait]
impl AiProvider for AmpProvider {
    fn metadata(&self) -> ProviderMetadata {
        ProviderMetadata {
            kind: ProviderKind::Amp,
            display_name: "Amp".into(),
            brand_name: "Amp".into(),
            icon_asset: "src/icons/provider-amp.svg".into(),
            dashboard_url: "https://ampcode.com/settings".into(),
            account_hint: "Amp CLI".into(),
            source_label: "amp cli".into(),
        }
    }

    fn id(&self) -> &'static str {
        "amp:cli"
    }

    async fn is_available(&self) -> bool {
        Command::new("amp").arg("--version").output().is_ok()
    }

    async fn refresh(&self) -> Result<Vec<QuotaInfo>> {
        let output = Command::new("amp")
            .args(["usage", "--no-color"])
            .output()
            .context("Failed to execute 'amp usage' correctly.")?;

        if !output.status.success() {
            anyhow::bail!(
                "'amp usage' command failed with exit status {:?}",
                output.status
            );
        }

        let output_str = String::from_utf8_lossy(&output.stdout);
        let mut quotas = Vec::new();

        for line in output_str.lines() {
            let line = line.trim();
            if let Some(caps) = CREDIT_RE.captures(line) {
                let label = caps.get(1).unwrap().as_str().trim();
                let remaining: f64 = caps.get(2).unwrap().as_str().parse().unwrap_or(0.0);
                let total: f64 = caps.get(3).unwrap().as_str().parse().unwrap_or(0.0);

                let used = total - remaining;
                quotas.push(QuotaInfo::new(label, used.max(0.0), total));
            } else if let Some(caps) = BALANCE_RE.captures(line) {
                let label = caps.get(1).unwrap().as_str().trim();
                let balance: f64 = caps.get(2).unwrap().as_str().parse().unwrap_or(0.0);

                quotas.push(QuotaInfo::new(label, 0.0, balance));
            }
        }

        if quotas.is_empty() {
            anyhow::bail!("No valid Amp usage data matched in the command output.");
        }

        Ok(quotas)
    }
}
