use super::{AiProvider, ProviderError};
use crate::models::{ProviderDescriptor, ProviderKind, ProviderMetadata, QuotaInfo, RefreshData};
use crate::providers::common::cli;
use anyhow::Result;
use async_trait::async_trait;
use regex::Regex;
use std::sync::LazyLock;

// 预编译的正则表达式
static EMAIL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Signed in as\s+(\S+)\s+\(").unwrap());
static CREDIT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(.+?):\s*\$([0-9]+(?:\.[0-9]+)?)\s*/\s*\$([0-9]+(?:\.[0-9]+)?)\s+remaining")
        .unwrap()
});
static BALANCE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(.+?):\s*\$([0-9]+(?:\.[0-9]+)?)\s+remaining").unwrap());

super::define_unit_provider!(AmpProvider);

impl AmpProvider {
    fn run_usage() -> Result<String> {
        let output = cli::run_checked_command("amp", &["usage", "--no-color"])?;
        Ok(cli::stdout_text(&output))
    }

    fn parse_usage_output(output_str: &str) -> Result<RefreshData> {
        let mut quotas = Vec::new();
        let mut account_email = None;

        for line in output_str.lines() {
            let line = line.trim();

            if account_email.is_none() {
                if let Some(caps) = EMAIL_RE.captures(line) {
                    account_email = Some(caps.get(1).unwrap().as_str().to_string());
                }
            }

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
            return Err(ProviderError::parse_failed(&format!(
                "cannot parse amp usage output:\n{}",
                output_str.trim()
            ))
            .into());
        }

        Ok(RefreshData::with_account(quotas, account_email, None))
    }
}

#[async_trait]
impl AiProvider for AmpProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: "amp:cli",
            metadata: ProviderMetadata {
                kind: ProviderKind::Amp,
                display_name: "Amp".into(),
                brand_name: "Amp".into(),
                icon_asset: "src/icons/provider-amp.svg".into(),
                dashboard_url: "https://ampcode.com/settings".into(),
                account_hint: "Amp CLI".into(),
                source_label: "amp cli".into(),
            },
        }
    }

    async fn check_availability(&self) -> Result<()> {
        if cli::command_exists("amp") {
            Ok(())
        } else {
            Err(ProviderError::cli_not_found("amp").into())
        }
    }

    async fn refresh(&self) -> Result<RefreshData> {
        let output = Self::run_usage()?;
        Self::parse_usage_output(&output)
    }
}
