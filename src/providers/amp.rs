use super::{AiProvider, ProviderError};
use crate::models::{
    ProviderDescriptor, ProviderKind, ProviderMetadata, QuotaInfo, QuotaType, RefreshData,
};
use crate::providers::common::cli;
use anyhow::Result;
use async_trait::async_trait;
use regex::Regex;
use rust_i18n::t;
use std::borrow::Cow;
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
                let detail = t!(
                    "quota.label.credit_remaining",
                    remaining = format!("{:.2}", remaining),
                    total = format!("{:.2}", total)
                )
                .to_string();
                quotas.push(QuotaInfo::with_details(
                    label,
                    used.max(0.0),
                    total,
                    QuotaType::General,
                    Some(detail),
                ));
            } else if let Some(caps) = BALANCE_RE.captures(line) {
                let label = caps.get(1).unwrap().as_str().trim();
                let balance: f64 = caps.get(2).unwrap().as_str().parse().unwrap_or(0.0);
                let detail = t!(
                    "quota.label.credit_remaining",
                    remaining = format!("{:.2}", balance),
                    total = format!("{:.2}", balance)
                )
                .to_string();
                quotas.push(QuotaInfo::with_details(
                    label,
                    0.0,
                    balance,
                    QuotaType::General,
                    Some(detail),
                ));
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
            id: Cow::Borrowed("amp:cli"),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_credit_with_total() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let output =
            "Signed in as user@example.com (Pro)\nMonthly credits: $15.00 / $20.00 remaining\n";
        let data = AmpProvider::parse_usage_output(output).unwrap();

        assert_eq!(data.account_email.as_deref(), Some("user@example.com"));
        assert_eq!(data.quotas.len(), 1);

        let q = &data.quotas[0];
        assert_eq!(q.label, "Monthly credits");
        assert_eq!(q.used, 5.0);
        assert_eq!(q.limit, 20.0);
        assert_eq!(q.quota_type, QuotaType::General);
        assert_eq!(q.detail_text.as_deref(), Some("$15.00 / $20.00 remaining"));
    }

    #[test]
    fn test_parse_balance_only() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let output = "Credits: $50.00 remaining\n";
        let data = AmpProvider::parse_usage_output(output).unwrap();

        assert_eq!(data.quotas.len(), 1);
        let q = &data.quotas[0];
        assert_eq!(q.label, "Credits");
        assert_eq!(q.used, 0.0);
        assert_eq!(q.limit, 50.0);
        assert_eq!(q.quota_type, QuotaType::General);
        assert_eq!(q.detail_text.as_deref(), Some("$50.00 / $50.00 remaining"));
    }

    #[test]
    fn test_parse_empty_output_returns_error() {
        assert!(AmpProvider::parse_usage_output("no match here").is_err());
    }
}
