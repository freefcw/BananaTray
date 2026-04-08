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
// "Amp Free: $10/$10 remaining (replenishes ...)" 或 "Monthly credits: $15.00 / $20.00 remaining"
// 后面可能跟 "(replenishes ...)" 或 "- https://..." 等附加文本。
static CREDIT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(.+?):\s*\$([0-9]+(?:\.[0-9]+)?)\s*/\s*\$([0-9]+(?:\.[0-9]+)?)\s+remaining")
        .unwrap()
});
// 无 total 的余额格式 — 必须放在 CREDIT_RE 之后作为回退：
// "Individual credits: $0 remaining" 或 "Credits: $50.00 remaining"
// 后面可能跟 url 或其他文本（如 "- https://..."），用 \s 终止金额匹配。
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
                    QuotaType::Credit,
                    Some(detail),
                ));
            } else if let Some(caps) = BALANCE_RE.captures(line) {
                let label = caps.get(1).unwrap().as_str().trim();
                let balance: f64 = caps.get(2).unwrap().as_str().parse().unwrap_or(0.0);

                // $0 remaining 表示"未购买付费信用额度"，不等同于"额度耗尽"，
                // 展示为 Red 会误导免费用户。跳过零余额条目。
                if balance <= 0.0 {
                    continue;
                }

                let detail = t!(
                    "quota.label.credit_remaining",
                    remaining = format!("{:.2}", balance),
                    total = format!("{:.2}", balance)
                )
                .to_string();
                // 使用 balance_only 模式：状态由余额绝对值决定（>=5 Green, >=1 Yellow, <1 Red），
                // 而非百分比——避免 limit=0 时 percent_remaining=0% 误判为 Red。
                quotas.push(QuotaInfo::balance_only(
                    label,
                    balance,
                    None,
                    QuotaType::Credit,
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
        assert_eq!(q.quota_type, QuotaType::Credit);
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
        assert!(q.is_balance_only());
        assert!((q.remaining_balance.unwrap() - 50.0).abs() < f64::EPSILON);
        assert_eq!(q.quota_type, QuotaType::Credit);
        assert_eq!(q.status_level(), crate::models::StatusLevel::Green);
    }

    /// 实际 amp CLI 输出：$10/$10 格式 + 零余额次要额度
    #[test]
    fn test_parse_real_world_free_tier() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let output = "Signed in as user@example.com (user)\n\
            Amp Free: $10/$10 remaining (replenishes +$0.42/hour) - https://ampcode.com/settings#amp-free\n\
            Individual credits: $0 remaining - https://ampcode.com/settings\n";
        let data = AmpProvider::parse_usage_output(output).unwrap();

        assert_eq!(data.account_email.as_deref(), Some("user@example.com"));
        // $0 余额的 Individual credits 应被跳过
        assert_eq!(data.quotas.len(), 1);
        let q = &data.quotas[0];
        assert_eq!(q.label, "Amp Free");
        assert_eq!(q.used, 0.0);
        assert_eq!(q.limit, 10.0);
        assert_eq!(q.status_level(), crate::models::StatusLevel::Green);
    }

    /// 零余额纯信用额度行应被跳过
    #[test]
    fn test_parse_zero_balance_only_is_skipped() {
        let output = "Individual credits: $0 remaining\n";
        assert!(
            AmpProvider::parse_usage_output(output).is_err(),
            "zero-balance-only output should produce no quotas → error"
        );
    }

    /// CREDIT_RE + 非零 BALANCE_RE 同时存在时都应产出 quota
    #[test]
    fn test_parse_mixed_credit_and_balance() {
        let _locale_guard = crate::i18n::test_locale_guard("en");
        let output = "Monthly credits: $5.00 / $20.00 remaining\n\
            Bonus credits: $3.00 remaining\n";
        let data = AmpProvider::parse_usage_output(output).unwrap();

        assert_eq!(data.quotas.len(), 2);
        assert_eq!(data.quotas[0].label, "Monthly credits");
        assert!(!data.quotas[0].is_balance_only());
        assert_eq!(data.quotas[1].label, "Bonus credits");
        assert!(data.quotas[1].is_balance_only());
        assert_eq!(
            data.quotas[1].status_level(),
            crate::models::StatusLevel::Yellow,
            "$3.00 should be Yellow (>=1 && <5)"
        );
    }

    #[test]
    fn test_parse_empty_output_returns_error() {
        assert!(AmpProvider::parse_usage_output("no match here").is_err());
    }
}
