//! Claude API Probe
//!
//! 通过 OAuth API 直接获取配额信息。
//! 无状态设计：每次 probe 从磁盘加载最新凭证，确保感知外部登录/登出变化。

use super::credentials::{refresh_oauth_token, save_credentials_atomic, ClaudeOAuthCredentials};
use super::probe::UsageProbe;
use crate::models::{QuotaInfo, QuotaType};
use crate::providers::common::http_client;
use crate::providers::ProviderError;
use crate::utils::time_utils;
use anyhow::{Context, Result};
use log::debug;
use rust_i18n::t;
use serde::Deserialize;

/// Claude API 获取方式（无状态）
pub struct ClaudeApiProbe;

impl ClaudeApiProbe {
    /// 加载凭证并获取有效的访问令牌
    ///
    /// 如果 Token 需要刷新，会自动刷新并原子写回凭证文件
    fn get_valid_token(creds: &mut ClaudeOAuthCredentials) -> Result<String> {
        if !creds.needs_refresh() {
            return Ok(creds.access_token.clone());
        }

        let rt = creds
            .refresh_token
            .as_deref()
            .context("missing refresh token, cannot refresh")?;

        debug!("Claude API: token needs refresh, refreshing...");
        let response = refresh_oauth_token(rt)?;
        creds.apply_refresh(&response);
        save_credentials_atomic(creds)?;
        debug!("Claude API: token refresh succeeded");
        Ok(creds.access_token.clone())
    }

    /// 调用 Usage API
    fn fetch_usage(access_token: &str) -> Result<UsageResponse> {
        const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";

        let auth_header = format!("Authorization: Bearer {}", access_token);

        let (body, status) = http_client::get_with_status(
            USAGE_URL,
            &[
                &auth_header,
                "Accept: application/json",
                "Content-Type: application/json",
                "anthropic-beta: oauth-2025-04-20",
            ],
        )?;

        let status_code: u16 = status.parse().unwrap_or(0);
        if status_code == 401 || status_code == 403 {
            return Err(ProviderError::session_expired(Some(&t!(
                "hint.relogin_cli",
                cli = "claude"
            )))
            .into());
        }
        if status_code >= 400 {
            return Err(ProviderError::fetch_failed(&t!(
                "hint.api_http_error",
                status = status_code
            ))
            .into());
        }

        let usage: UsageResponse =
            serde_json::from_str(&body).with_context(|| "cannot parse Usage API response")?;

        Ok(usage)
    }

    /// 将单个配额数据转换为 QuotaInfo
    fn push_percent_quota(
        quotas: &mut Vec<QuotaInfo>,
        data: Option<UsageQuotaData>,
        label: &str,
        kind: QuotaType,
    ) {
        if let Some(d) = data {
            if let Some(utilization) = d.utilization {
                let reset_at = d.resets_at.as_ref().and_then(|s| {
                    time_utils::parse_iso8601_to_epoch(s).map(time_utils::format_reset_from_epoch)
                });
                quotas.push(QuotaInfo::with_details(
                    label,
                    utilization,
                    100.0,
                    kind,
                    reset_at,
                ));
            }
        }
    }

    /// 解析 Usage 响应为 QuotaInfo 列表
    fn parse_usage(usage: UsageResponse) -> Vec<QuotaInfo> {
        let mut quotas = Vec::new();

        let session_label = t!("quota.label.session").to_string();
        Self::push_percent_quota(
            &mut quotas,
            usage.five_hour,
            &session_label,
            QuotaType::Session,
        );
        let weekly_label = t!("quota.label.weekly").to_string();
        Self::push_percent_quota(
            &mut quotas,
            usage.seven_day,
            &weekly_label,
            QuotaType::Weekly,
        );
        let sonnet_label = t!("quota.label.weekly_model", model = "Sonnet").to_string();
        Self::push_percent_quota(
            &mut quotas,
            usage.seven_day_sonnet,
            &sonnet_label,
            QuotaType::ModelSpecific("Sonnet".to_string()),
        );
        let opus_label = t!("quota.label.weekly_model", model = "Opus").to_string();
        Self::push_percent_quota(
            &mut quotas,
            usage.seven_day_opus,
            &opus_label,
            QuotaType::ModelSpecific("Opus".to_string()),
        );

        // 额外用量（付费）
        if let Some(extra) = usage.extra_usage {
            if extra.is_enabled == Some(true) {
                if let (Some(used_credits), Some(monthly_limit)) =
                    (extra.used_credits, extra.monthly_limit)
                {
                    quotas.push(QuotaInfo::with_details(
                        t!("quota.label.extra_usage").to_string(),
                        used_credits / 100.0,
                        monthly_limit / 100.0,
                        QuotaType::Credit,
                        None,
                    ));
                }
            }
        }

        quotas
    }
}

impl UsageProbe for ClaudeApiProbe {
    fn probe(&self) -> Result<Vec<QuotaInfo>> {
        let mut creds = ClaudeOAuthCredentials::load()?;
        let access_token = Self::get_valid_token(&mut creds)?;
        let usage = Self::fetch_usage(&access_token)?;
        let quotas = Self::parse_usage(usage);

        if quotas.is_empty() {
            return Err(ProviderError::no_data().into());
        }

        Ok(quotas)
    }

    fn is_available(&self) -> bool {
        ClaudeOAuthCredentials::try_load().is_some()
    }
}

// ============================================================================
// API 响应模型
// ============================================================================

#[derive(Debug, Deserialize)]
struct UsageResponse {
    five_hour: Option<UsageQuotaData>,
    seven_day: Option<UsageQuotaData>,
    seven_day_sonnet: Option<UsageQuotaData>,
    seven_day_opus: Option<UsageQuotaData>,
    extra_usage: Option<ExtraUsageData>,
}

#[derive(Debug, Deserialize)]
struct UsageQuotaData {
    utilization: Option<f64>,
    resets_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ExtraUsageData {
    is_enabled: Option<bool>,
    used_credits: Option<f64>,
    monthly_limit: Option<f64>,
}
