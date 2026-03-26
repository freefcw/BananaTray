use super::http_client;
use super::time_utils;
use super::AiProvider;
use crate::models::{ProviderKind, QuotaInfo, QuotaType};
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use std::process::Command;

pub struct KimiProvider {}

impl KimiProvider {
    pub fn new() -> Self {
        Self {}
    }

    fn get_token(&self) -> Option<String> {
        std::env::var("KIMI_AUTH_TOKEN")
            .ok()
            .filter(|t| !t.is_empty())
    }

    fn fetch_quota_via_api(&self, token: &str) -> Result<Vec<QuotaInfo>> {
        let auth_header = format!("Authorization: Bearer {}", token);
        let cookie_header = format!("Cookie: kimi-auth={}", token);

        let response_str = http_client::curl_post_json(
            "https://www.kimi.com/apiv2/kimi.gateway.billing.v1.BillingService/GetUsages",
            &[
                &auth_header,
                &cookie_header,
                "Origin: https://www.kimi.com",
                "Referer: https://www.kimi.com/code/console",
                "Accept: */*",
                "User-Agent: Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36",
                "connect-protocol-version: 1",
                "x-language: en-US",
                "x-msh-platform: web",
            ],
            r#"{"scope":["FEATURE_CODING"]}"#,
        )?;

        let resp: KimiUsageResponse = serde_json::from_str(&response_str).with_context(|| {
            format!(
                "Failed to parse Kimi API response: {}",
                response_str.chars().take(200).collect::<String>()
            )
        })?;

        let usages = resp.usages.unwrap_or_default();

        let coding_usage = usages
            .iter()
            .find(|u| u.scope.as_deref() == Some("FEATURE_CODING"))
            .context("No FEATURE_CODING usage data found.")?;

        let mut quotas = Vec::new();

        // Weekly quota from top-level detail
        if let Some(detail) = &coding_usage.detail {
            let used = parse_num(&detail.used);
            let limit = parse_num(&detail.limit);
            let tier = detect_tier(limit);
            let label = match tier {
                Some(t) => format!("Weekly ({})", t),
                None => "Weekly".to_string(),
            };
            let reset_at = detail
                .reset_time
                .as_deref()
                .and_then(time_utils::format_reset_countdown);

            quotas.push(QuotaInfo::with_details(
                label,
                used,
                limit,
                QuotaType::Weekly,
                reset_at,
            ));
        }

        // Session (5h) quota from limits where duration == 300 && timeUnit == TIME_UNIT_MINUTE
        if let Some(limits) = &coding_usage.limits {
            for lim in limits {
                let is_5h_window = lim
                    .window
                    .as_ref()
                    .map(|w| {
                        w.duration == Some(300)
                            && w.time_unit.as_deref() == Some("TIME_UNIT_MINUTE")
                    })
                    .unwrap_or(false);

                if is_5h_window {
                    if let Some(detail) = &lim.detail {
                        let used = parse_num(&detail.used);
                        let limit = parse_num(&detail.limit);
                        let reset_at = detail
                            .reset_time
                            .as_deref()
                            .and_then(time_utils::format_reset_countdown);

                        quotas.push(QuotaInfo::with_details(
                            "Session (5h)",
                            used,
                            limit,
                            QuotaType::Session,
                            reset_at,
                        ));
                    }
                }
            }
        }

        if quotas.is_empty() {
            anyhow::bail!("No FEATURE_CODING usage data found.");
        }

        Ok(quotas)
    }
}

#[async_trait]
impl AiProvider for KimiProvider {
    fn id(&self) -> &'static str {
        "kimi:api"
    }

    fn kind(&self) -> ProviderKind {
        ProviderKind::Kimi
    }

    async fn is_available(&self) -> bool {
        if self.get_token().is_some() {
            return true;
        }
        Command::new("kimi").arg("--version").output().is_ok()
    }

    async fn refresh(&self) -> Result<Vec<QuotaInfo>> {
        if let Some(token) = self.get_token() {
            return self.fetch_quota_via_api(&token);
        }

        // Fallback: check if CLI exists but we can't use it for quota
        if Command::new("kimi").arg("--version").output().is_ok() {
            anyhow::bail!("Set KIMI_AUTH_TOKEN environment variable to enable Kimi monitoring.");
        }

        anyhow::bail!("Set KIMI_AUTH_TOKEN environment variable to enable Kimi monitoring.")
    }
}

// --- Serde structures ---

#[derive(Deserialize)]
struct KimiUsageResponse {
    usages: Option<Vec<KimiUsage>>,
}

#[derive(Deserialize)]
struct KimiUsage {
    scope: Option<String>,
    detail: Option<KimiUsageDetail>,
    limits: Option<Vec<KimiLimit>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct KimiUsageDetail {
    limit: Option<String>,
    used: Option<String>,
    #[allow(dead_code)]
    remaining: Option<String>,
    reset_time: Option<String>,
}

#[derive(Deserialize)]
struct KimiLimit {
    window: Option<KimiWindow>,
    detail: Option<KimiUsageDetail>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct KimiWindow {
    duration: Option<u64>,
    time_unit: Option<String>,
}

// --- Helpers ---

fn parse_num(val: &Option<String>) -> f64 {
    val.as_deref()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0)
}

fn detect_tier(weekly_limit: f64) -> Option<&'static str> {
    match weekly_limit as u64 {
        1024 => Some("Andante"),
        2048 => Some("Moderato"),
        7168 => Some("Allegretto"),
        _ => None,
    }
}
