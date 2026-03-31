use super::{AiProvider, ProviderError};
use crate::models::{ProviderKind, ProviderMetadata, QuotaInfo, QuotaType};
use crate::utils::interactive_runner::{InteractiveOptions, InteractiveRunner};
use crate::utils::text_utils;
use anyhow::{Context, Result};
use async_trait::async_trait;
use regex::Regex;
use std::process::Command;
use std::sync::LazyLock;
use std::time::Duration;

// 预编译的正则表达式
static CREDITS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Credits \(([0-9.]+) of ([0-9.]+) covered in plan\)").unwrap());
static RESET_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"resets on (\d{4}-\d{2}-\d{2})").unwrap());

super::define_unit_provider!(KiroProvider);

impl KiroProvider {
    /// Run `kiro-cli chat` interactively and send `/usage` command.
    ///
    /// kiro-cli chat initializes MCP servers first (shows a spinner for several
    /// seconds), then displays a prompt. We must wait for that init to finish
    /// before sending `/usage`, otherwise the command gets lost.
    fn run_kiro_cli() -> Result<String> {
        let start = std::time::Instant::now();
        log::info!(target: "providers::kiro", "Starting kiro-cli chat for /usage");

        let runner = InteractiveRunner::new();

        // Phase 1: Start `kiro-cli chat` with no input, wait for MCP init to finish.
        // The ready signal is the chat prompt (e.g. "(/usage for more detail)").
        // We use a long init but short idle timeout so the spinner doesn't block.
        let mut opts_init = InteractiveOptions {
            timeout: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(2),
            init_delay: Duration::from_millis(200),
            arguments: vec!["chat".to_string()],
            ..Default::default()
        };
        // Don't send input yet - let the MCP init complete first
        // Use auto_response: when the prompt line appears, send /usage
        opts_init
            .auto_responses
            .insert("/usage for more detail".to_string(), "/usage\r".to_string());

        let result = runner
            .run("kiro-cli", "", opts_init)
            .context("Failed to run kiro-cli")?;

        log::info!(
            target: "providers::kiro",
            "kiro-cli completed in {:.2}s, exit_code={:?}, output_len={}, output_preview={:?}",
            start.elapsed().as_secs_f64(),
            result.exit_code,
            result.output.len(),
            &result.output[..result.output.len().min(500)]
        );

        Ok(result.output)
    }

    /// Parse the output of `kiro-cli /usage` into quota entries.
    ///
    /// Sample output:
    /// ```text
    /// Estimated Usage | resets on 2026-04-01 | KIRO FREE
    /// Credits (5.13 of 50 covered in plan)
    /// ████████████████ 10%
    /// ```
    fn parse_usage_output(raw: &str) -> Result<Vec<QuotaInfo>> {
        let clean = text_utils::strip_ansi(raw);
        let mut quotas = Vec::new();

        // Parse credits
        if let Some(caps) = CREDITS_RE.captures(&clean) {
            let used: f64 = caps[1].parse().unwrap_or(0.0);
            let total: f64 = caps[2].parse().unwrap_or(0.0);

            let reset_text = RESET_RE
                .captures(&clean)
                .map(|c| format!("Resets on {}", &c[1]));

            if total > 0.0 {
                quotas.push(QuotaInfo::with_details(
                    "Credits",
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

    async fn refresh_quotas(&self) -> Result<Vec<QuotaInfo>> {
        let start = std::time::Instant::now();
        let stdout = Self::run_kiro_cli()?;

        let quotas = Self::parse_usage_output(&stdout)?;
        log::info!(
            target: "providers::kiro",
            "Parsed {} quota(s) in {:.2}s total",
            quotas.len(),
            start.elapsed().as_secs_f64()
        );

        if quotas.is_empty() {
            return Err(ProviderError::parse_failed(&format!(
                "无法解析 kiro-cli 输出:\n{}",
                stdout.trim()
            ))
            .into());
        }

        Ok(quotas)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_OUTPUT: &str = r#"
Estimated Usage | resets on 2026-04-01 | KIRO FREE
Credits (5.13 of 50 covered in plan)
████████████████ 10%

Overages: Disabled
"#;

    #[test]
    fn test_parse_credits() {
        let quotas = KiroProvider::parse_usage_output(SAMPLE_OUTPUT).unwrap();
        assert_eq!(quotas.len(), 1);

        let credits = &quotas[0];
        assert_eq!(credits.label, "Credits");
        assert!((credits.used - 5.13).abs() < 0.01);
        assert!((credits.limit - 50.0).abs() < 0.01);
        assert_eq!(credits.quota_type, QuotaType::General);
        assert_eq!(credits.reset_at.as_deref(), Some("Resets on 2026-04-01"));
    }

    #[test]
    fn test_parse_with_ansi_codes() {
        let output = "\x1b[32mEstimated Usage | resets on 2026-05-15 | KIRO FREE\x1b[0m\nCredits (25.50 of 100 covered in plan)\n████████████ 26%\n";
        let quotas = KiroProvider::parse_usage_output(output).unwrap();
        assert_eq!(quotas.len(), 1);

        assert!((quotas[0].used - 25.50).abs() < 0.01);
        assert!((quotas[0].limit - 100.0).abs() < 0.01);
        assert_eq!(quotas[0].reset_at.as_deref(), Some("Resets on 2026-05-15"));
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
