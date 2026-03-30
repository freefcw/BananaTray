use super::AiProvider;
use crate::models::{ProviderKind, ProviderMetadata, QuotaInfo, QuotaType};
use crate::utils::text_utils;
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use regex::Regex;
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::LazyLock;
use std::time::{Duration, Instant};

// 预编译的正则表达式
static BONUS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Bonus credits:\s*([\d.]+)/([\d.]+)\s*credits used").unwrap());
static EXPIRY_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"expires in (\d+) days").unwrap());
static CREDITS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Credits \(([\d.]+) of ([\d.]+)").unwrap());
static KIRO_RESET_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"resets on (\d{2}/\d{2})").unwrap());

super::define_unit_provider!(KiroProvider);

impl KiroProvider {
    /// Run `kiro-cli` interactively, sending `/usage` and `/quit` via stdin,
    /// and return the combined stdout output.
    fn run_kiro_cli() -> Result<String> {
        let mut child = Command::new("kiro-cli")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Failed to spawn 'kiro-cli'. Is Kiro CLI installed?")?;

        // Write commands to stdin
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(b"/usage\n/quit\n")
                .context("Failed to write to kiro-cli stdin")?;
        }

        // Wait for process with timeout
        let timeout = Duration::from_secs(30);
        let start = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(_status)) => break,
                Ok(None) => {
                    if start.elapsed() > timeout {
                        let _ = child.kill();
                        bail!("kiro-cli timed out after 30 seconds");
                    }
                    std::thread::sleep(Duration::from_millis(200));
                }
                Err(e) => bail!("Error waiting for kiro-cli: {}", e),
            }
        }

        let output = child
            .wait_with_output()
            .context("Failed to read kiro-cli output")?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        Ok(stdout)
    }

    /// Parse the output of `kiro-cli /usage` into quota entries.
    ///
    /// Sample output:
    /// ```text
    /// Estimated Usage | resets on 03/01 | KIRO FREE
    ///
    /// 🎁 Bonus credits: 122.54/500 credits used, expires in 29 days
    ///
    /// Credits (0.00 of 50 covered in plan)
    /// ████████████████████████████████████████████████████████████████ 0%
    /// ```
    fn parse_usage_output(raw: &str) -> Result<Vec<QuotaInfo>> {
        let clean = text_utils::strip_ansi(raw);
        let mut quotas = Vec::new();

        // Parse bonus credits
        if let Some(caps) = BONUS_RE.captures(&clean) {
            let used: f64 = caps[1].parse().unwrap_or(0.0);
            let total: f64 = caps[2].parse().unwrap_or(0.0);

            let reset_text = EXPIRY_RE
                .captures(&clean)
                .map(|c| format!("Expires in {} days", &c[1]));

            if total > 0.0 {
                quotas.push(QuotaInfo::with_details(
                    "Bonus Credits",
                    used,
                    total,
                    QuotaType::Weekly,
                    reset_text,
                ));
            }
        }

        // Parse regular credits
        if let Some(caps) = CREDITS_RE.captures(&clean) {
            let used: f64 = caps[1].parse().unwrap_or(0.0);
            let total: f64 = caps[2].parse().unwrap_or(0.0);

            let reset_text = KIRO_RESET_RE
                .captures(&clean)
                .map(|c| format!("Resets on {}", &c[1]));

            if total > 0.0 {
                quotas.push(QuotaInfo::with_details(
                    "Monthly Credits",
                    used,
                    total,
                    QuotaType::General,
                    reset_text,
                ));
            }
        }

        Ok(quotas)
    }
}

#[async_trait]
impl AiProvider for KiroProvider {
    fn metadata(&self) -> ProviderMetadata {
        ProviderMetadata {
            kind: ProviderKind::Kiro,
            display_name: "Kiro".into(),
            brand_name: "AWS".into(),
            icon_asset: "src/icons/provider-kiro.svg".into(),
            dashboard_url: "https://app.kiro.dev/account/usage".into(),
            account_hint: "AWS account".into(),
            source_label: "kiro cli".into(),
        }
    }

    fn id(&self) -> &'static str {
        "kiro:cli"
    }

    async fn is_available(&self) -> bool {
        Command::new("kiro-cli").arg("--version").output().is_ok()
    }

    async fn refresh(&self) -> Result<Vec<QuotaInfo>> {
        let stdout = Self::run_kiro_cli()?;

        let quotas = Self::parse_usage_output(&stdout)?;

        if quotas.is_empty() {
            bail!(
                "No quota data found in kiro-cli /usage output. Raw output:\n{}",
                stdout.trim()
            );
        }

        Ok(quotas)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_OUTPUT: &str = r#"
Estimated Usage | resets on 03/01 | KIRO FREE

🎁 Bonus credits: 122.54/500 credits used, expires in 29 days

Credits (0.00 of 50 covered in plan)
████████████████████████████████████████████████████████████████████████████████ 0%
"#;

    #[test]
    fn test_parse_bonus_credits() {
        let quotas = KiroProvider::parse_usage_output(SAMPLE_OUTPUT).unwrap();
        assert!(!quotas.is_empty());

        let bonus = &quotas[0];
        assert_eq!(bonus.label, "Bonus Credits");
        assert!((bonus.used - 122.54).abs() < 0.01);
        assert!((bonus.limit - 500.0).abs() < 0.01);
        assert_eq!(bonus.quota_type, QuotaType::Weekly);
        assert_eq!(bonus.reset_at.as_deref(), Some("Expires in 29 days"));
    }

    #[test]
    fn test_parse_monthly_credits() {
        let quotas = KiroProvider::parse_usage_output(SAMPLE_OUTPUT).unwrap();
        assert_eq!(quotas.len(), 2);

        let monthly = &quotas[1];
        assert_eq!(monthly.label, "Monthly Credits");
        assert!((monthly.used - 0.0).abs() < 0.01);
        assert!((monthly.limit - 50.0).abs() < 0.01);
        assert_eq!(monthly.quota_type, QuotaType::General);
        assert_eq!(monthly.reset_at.as_deref(), Some("Resets on 03/01"));
    }

    #[test]
    fn test_parse_with_ansi_codes() {
        let output = "\x1b[32mEstimated Usage | resets on 04/15 | KIRO FREE\x1b[0m\n\n\x1b[33m🎁 Bonus credits: 200.00/500 credits used, expires in 10 days\x1b[0m\n\nCredits (25.50 of 100 covered in plan)\n████████████ 26%\n";
        let quotas = KiroProvider::parse_usage_output(output).unwrap();
        assert_eq!(quotas.len(), 2);

        assert!((quotas[0].used - 200.0).abs() < 0.01);
        assert!((quotas[1].used - 25.50).abs() < 0.01);
        assert!((quotas[1].limit - 100.0).abs() < 0.01);
        assert_eq!(quotas[1].reset_at.as_deref(), Some("Resets on 04/15"));
    }

    #[test]
    fn test_parse_no_quota_data() {
        let quotas = KiroProvider::parse_usage_output("some random output").unwrap();
        assert!(quotas.is_empty());
    }

    #[test]
    fn test_strip_ansi() {
        let input = "\x1b[31mhello\x1b[0m world";
        let clean = text_utils::strip_ansi(input);
        assert_eq!(clean, "hello world");
    }
}
