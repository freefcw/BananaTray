use super::{AiProvider, ProviderError};
use crate::models::{
    ProviderDescriptor, ProviderKind, ProviderMetadata, QuotaInfo, QuotaType, RefreshData,
};
use crate::providers::common::cli;
use crate::utils::text_utils;
use anyhow::Result;
use async_trait::async_trait;
use regex::Regex;
use rust_i18n::t;
use std::sync::LazyLock;

const KIRO_CLI: &str = "kiro-cli";

// 预编译的正则表达式
static CREDITS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Credits \(([0-9.]+) of ([0-9.]+) covered in plan\)").unwrap());
// "Bonus credits: 122.54/500 credits used, expires in 29 days"
static BONUS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"Bonus credits:\s*([0-9.]+)/([0-9.]+)\s*credits used,\s*expires in (\d+) days")
        .unwrap()
});
// "resets on 2026-04-01" or "resets on 03/01"
static RESET_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"resets on (\d{2,4}[-/]\d{2}(?:[-/]\d{2})?)").unwrap());
static TIER_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\|\s*(KIRO\s+\w+)").unwrap());
static WHOAMI_EMAIL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^Email:\s*(.+)$").unwrap());

super::define_unit_provider!(KiroProvider);

impl KiroProvider {
    /// 执行 `kiro-cli chat --no-interactive /usage` 获取用量。
    ///
    /// `kiro-cli` 会在不同版本里把正文写到 stdout 或 stderr，这里统一收敛。
    fn run_usage() -> Result<String> {
        let start = std::time::Instant::now();
        log::info!(target: "providers::kiro", "Running kiro-cli chat --no-interactive /usage");

        let output = cli::run_command(KIRO_CLI, &["chat", "--no-interactive", "/usage"])?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        log::info!(
            target: "providers::kiro",
            "kiro-cli completed in {:.2}s, exit_code={}, stdout_len={}, stderr_len={}, using={}",
            start.elapsed().as_secs_f64(),
            output.status,
            stdout.len(),
            stderr.len(),
            if stdout.trim().is_empty() { "stderr" } else { "stdout" },
        );

        cli::ensure_success(&output)?;
        Ok(cli::stdout_or_stderr_text(&output))
    }

    /// 执行 `kiro-cli whoami` 读取当前登录邮箱。
    fn read_account_email() -> Option<String> {
        let output = cli::run_command(KIRO_CLI, &["whoami"]).ok()?;

        if !output.status.success() {
            log::warn!(target: "providers::kiro", "kiro-cli whoami failed: {:?}", output.status);
            return None;
        }

        let stdout = cli::stdout_text(&output);
        Self::parse_whoami_email(&stdout)
    }

    /// 从 `kiro-cli whoami` 输出中提取邮箱。
    fn parse_whoami_email(raw: &str) -> Option<String> {
        for line in raw.lines() {
            let line = line.trim();
            if let Some(caps) = WHOAMI_EMAIL_RE.captures(line) {
                let email = caps[1].trim().to_string();
                if !email.is_empty() {
                    return Some(email);
                }
            }
        }
        None
    }

    /// 解析 `kiro-cli chat --no-interactive /usage` 输出。
    ///
    /// 支持两类配额：
    /// - 常规 credits: `Credits (12.39 of 50 covered in plan)`
    /// - Bonus credits: `Bonus credits: 122.54/500 credits used, expires in 29 days`
    fn parse_usage_output(raw: &str) -> Result<Vec<QuotaInfo>> {
        let clean = text_utils::strip_terminal_noise(raw);
        let mut quotas = Vec::new();

        let reset_text = RESET_RE
            .captures(&clean)
            .map(|c| t!("quota.label.resets_on", date = &c[1]).to_string());

        // Bonus credits
        if let Some(caps) = BONUS_RE.captures(&clean) {
            let used: f64 = caps[1].parse().unwrap_or(0.0);
            let total: f64 = caps[2].parse().unwrap_or(0.0);
            let days: u32 = caps[3].parse().unwrap_or(0);

            if total > 0.0 {
                let expiry = t!("quota.label.expires_in_days", days = days).to_string();
                quotas.push(QuotaInfo::with_details(
                    t!("quota.label.bonus_credits").to_string(),
                    used,
                    total,
                    QuotaType::Credit,
                    Some(expiry),
                ));
            }
        }

        // Regular credits
        if let Some(caps) = CREDITS_RE.captures(&clean) {
            let used: f64 = caps[1].parse().unwrap_or(0.0);
            let total: f64 = caps[2].parse().unwrap_or(0.0);

            if total > 0.0 {
                quotas.push(QuotaInfo::with_details(
                    t!("quota.label.credits").to_string(),
                    used,
                    total,
                    QuotaType::General,
                    reset_text,
                ));
            }
        }

        Ok(quotas)
    }

    /// 从 usage 输出提取账户层级，例如 `KIRO FREE`、`KIRO PRO`。
    fn parse_account_tier(raw: &str) -> Option<String> {
        let clean = text_utils::strip_terminal_noise(raw);
        for line in clean.lines() {
            if let Some(caps) = TIER_RE.captures(line.trim()) {
                return Some(caps[1].trim().to_string());
            }
        }
        None
    }
}

#[async_trait]
impl AiProvider for KiroProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: "kiro:cli",
            metadata: ProviderMetadata {
                kind: ProviderKind::Kiro,
                display_name: "Kiro".into(),
                brand_name: "AWS".into(),
                icon_asset: "src/icons/provider-kiro.svg".into(),
                dashboard_url: "https://app.kiro.dev/account/usage".into(),
                account_hint: "AWS account".into(),
                source_label: "kiro cli".into(),
            },
        }
    }

    async fn check_availability(&self) -> Result<()> {
        if cli::command_exists(KIRO_CLI) {
            Ok(())
        } else {
            Err(ProviderError::cli_not_found(KIRO_CLI).into())
        }
    }

    async fn refresh(&self) -> Result<RefreshData> {
        let start = std::time::Instant::now();
        let stdout = Self::run_usage()?;

        let quotas = Self::parse_usage_output(&stdout)?;
        let account_tier = Self::parse_account_tier(&stdout);
        let account_email = Self::read_account_email();

        log::info!(
            target: "providers::kiro",
            "Parsed {} quota(s) in {:.2}s, tier={:?}, email={:?}",
            quotas.len(),
            start.elapsed().as_secs_f64(),
            account_tier,
            account_email,
        );

        if quotas.is_empty() {
            return Err(ProviderError::parse_failed(&format!(
                "cannot parse kiro-cli output:\n{}",
                stdout.trim()
            ))
            .into());
        }

        Ok(RefreshData::with_account(
            quotas,
            account_email,
            account_tier,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_OUTPUT: &str = "\
\x1b[1mEstimated Usage\x1b[0m | resets on 2026-04-01 | \x1b[38;5;141mKIRO FREE\x1b[0m
\x1b[1mCredits\x1b[0m (12.39 of 50 covered in plan)
\x1b[38;5;141m███████████████████\x1b[38;5;244m█████████████████████████████████████████████████████████████\x1b[0m 24%

Overages: \x1b[1mDisabled\x1b[0m

To manage your plan or configure overages navigate to \x1b[38;5;141mhttps://app.kiro.dev/account/usage\x1b[0m
";

    const SAMPLE_WITH_BONUS: &str = "\
Estimated Usage | resets on 03/01 | KIRO FREE

\u{1f381} Bonus credits: 122.54/500 credits used, expires in 29 days

Credits (0.00 of 50 covered in plan)
\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588} 0%

Overages: Disabled
";

    #[test]
    fn test_parse_credits_from_real_output() {
        let quotas = KiroProvider::parse_usage_output(SAMPLE_OUTPUT).unwrap();
        assert_eq!(quotas.len(), 1);

        let credits = &quotas[0];
        assert_eq!(credits.label, "Credits");
        assert!((credits.used - 12.39).abs() < 0.01);
        assert!((credits.limit - 50.0).abs() < 0.01);
        assert_eq!(credits.quota_type, QuotaType::General);
        assert_eq!(credits.reset_at.as_deref(), Some("Resets on 2026-04-01"));
    }

    #[test]
    fn test_parse_credits_plain() {
        let output = "Estimated Usage | resets on 2026-05-15 | KIRO FREE\nCredits (25.50 of 100 covered in plan)\n";
        let quotas = KiroProvider::parse_usage_output(output).unwrap();
        assert_eq!(quotas.len(), 1);
        assert!((quotas[0].used - 25.50).abs() < 0.01);
        assert!((quotas[0].limit - 100.0).abs() < 0.01);
        assert_eq!(quotas[0].reset_at.as_deref(), Some("Resets on 2026-05-15"));
    }

    #[test]
    fn test_parse_reset_date_mm_dd() {
        let output =
            "Estimated Usage | resets on 03/01 | KIRO FREE\nCredits (5.0 of 50 covered in plan)\n";
        let quotas = KiroProvider::parse_usage_output(output).unwrap();
        assert_eq!(quotas.len(), 1);
        assert_eq!(quotas[0].reset_at.as_deref(), Some("Resets on 03/01"));
    }

    #[test]
    fn test_parse_bonus_and_regular_credits() {
        let quotas = KiroProvider::parse_usage_output(SAMPLE_WITH_BONUS).unwrap();
        assert_eq!(quotas.len(), 2);

        let bonus = &quotas[0];
        assert_eq!(bonus.label, "Bonus Credits");
        assert!((bonus.used - 122.54).abs() < 0.01);
        assert!((bonus.limit - 500.0).abs() < 0.01);
        assert_eq!(bonus.quota_type, QuotaType::Credit);
        assert_eq!(bonus.reset_at.as_deref(), Some("Expires in 29 days"));

        let regular = &quotas[1];
        assert_eq!(regular.label, "Credits");
        assert!((regular.used - 0.0).abs() < 0.01);
        assert!((regular.limit - 50.0).abs() < 0.01);
        assert_eq!(regular.quota_type, QuotaType::General);
        assert_eq!(regular.reset_at.as_deref(), Some("Resets on 03/01"));
    }

    #[test]
    fn test_parse_bonus_only() {
        let output = "Bonus credits: 10.5/100 credits used, expires in 5 days\n";
        let quotas = KiroProvider::parse_usage_output(output).unwrap();
        assert_eq!(quotas.len(), 1);
        assert_eq!(quotas[0].label, "Bonus Credits");
        assert!((quotas[0].used - 10.5).abs() < 0.01);
        assert!((quotas[0].limit - 100.0).abs() < 0.01);
        assert_eq!(quotas[0].reset_at.as_deref(), Some("Expires in 5 days"));
    }

    #[test]
    fn test_parse_no_quota_data() {
        let quotas = KiroProvider::parse_usage_output("some random output").unwrap();
        assert!(quotas.is_empty());
    }

    #[test]
    fn test_parse_tier_from_real_output() {
        let tier = KiroProvider::parse_account_tier(SAMPLE_OUTPUT);
        assert_eq!(tier.as_deref(), Some("KIRO FREE"));
    }

    #[test]
    fn test_parse_tier_pro() {
        let output = "Estimated Usage | resets on 2026-04-01 | KIRO PRO\nCredits (10.0 of 200 covered in plan)\n";
        let tier = KiroProvider::parse_account_tier(output);
        assert_eq!(tier.as_deref(), Some("KIRO PRO"));
    }

    #[test]
    fn test_parse_tier_with_ansi() {
        let output = "Estimated Usage | resets on 2026-04-01 | \x1b[38;5;141mKIRO FREE\x1b[0m\n";
        let tier = KiroProvider::parse_account_tier(output);
        assert_eq!(tier.as_deref(), Some("KIRO FREE"));
    }

    #[test]
    fn test_parse_tier_not_found() {
        let tier = KiroProvider::parse_account_tier("some random output");
        assert!(tier.is_none());
    }

    #[test]
    fn test_parse_whoami_email() {
        let output = "Logged in with GitHub\nEmail: freefcw@gmail.com\n";
        let email = KiroProvider::parse_whoami_email(output);
        assert_eq!(email.as_deref(), Some("freefcw@gmail.com"));
    }

    #[test]
    fn test_parse_whoami_email_with_spaces() {
        let output = "Logged in with GitHub\n  Email:   user@example.com  \n";
        let email = KiroProvider::parse_whoami_email(output);
        assert_eq!(email.as_deref(), Some("user@example.com"));
    }

    #[test]
    fn test_parse_whoami_no_email() {
        let email = KiroProvider::parse_whoami_email("Not logged in\n");
        assert!(email.is_none());
    }
}
