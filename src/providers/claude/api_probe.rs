//! Claude API Probe
//!
//! 通过 OAuth API 直接获取配额信息。
//! 无状态设计：每次 probe 从磁盘加载最新凭证，确保感知外部登录/登出变化。

use super::credentials::{refresh_oauth_token, save_credentials_atomic, ClaudeOAuthCredentials};
use super::probe::UsageProbe;
use crate::models::{QuotaDetailSpec, QuotaInfo, QuotaLabelSpec, QuotaType};
use crate::providers::common::http_client;
use crate::providers::ProviderError;
use crate::utils::time_utils;
use anyhow::{Context, Result};
use log::debug;
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
    ///
    /// 4xx/5xx → `HttpError::HttpStatus`（由 http_client 层自动返回），
    /// 上层通过 `ProviderError::classify` 自动分类为 AuthRequired / FetchFailed 等。
    fn fetch_usage(access_token: &str) -> Result<UsageResponse> {
        const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";

        let auth_header = format!("Authorization: Bearer {}", access_token);

        let body = http_client::get(
            USAGE_URL,
            &[
                &auth_header,
                "Accept: application/json",
                "Content-Type: application/json",
                "anthropic-beta: oauth-2025-04-20",
            ],
        )?;

        let usage: UsageResponse =
            serde_json::from_str(&body).with_context(|| "cannot parse Usage API response")?;

        Ok(usage)
    }

    /// 将单个配额数据转换为 QuotaInfo
    fn push_percent_quota(
        quotas: &mut Vec<QuotaInfo>,
        data: Option<UsageQuotaData>,
        label: QuotaLabelSpec,
        kind: QuotaType,
    ) {
        if let Some(d) = data {
            if let Some(utilization) = d.utilization {
                let reset_at = d.resets_at.as_ref().and_then(|s| {
                    time_utils::parse_iso8601_to_epoch(s)
                        .map(|epoch_secs| QuotaDetailSpec::ResetAt { epoch_secs })
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

        Self::push_percent_quota(
            &mut quotas,
            usage.five_hour,
            QuotaLabelSpec::Session,
            QuotaType::Session,
        );
        Self::push_percent_quota(
            &mut quotas,
            usage.seven_day,
            QuotaLabelSpec::Weekly,
            QuotaType::Weekly,
        );
        Self::push_percent_quota(
            &mut quotas,
            usage.seven_day_sonnet,
            QuotaLabelSpec::WeeklyModel {
                model: "Sonnet".to_string(),
            },
            QuotaType::ModelSpecific("Sonnet".to_string()),
        );
        Self::push_percent_quota(
            &mut quotas,
            usage.seven_day_opus,
            QuotaLabelSpec::WeeklyModel {
                model: "Opus".to_string(),
            },
            QuotaType::ModelSpecific("Opus".to_string()),
        );

        // 额外用量（付费）
        if let Some(extra) = usage.extra_usage {
            if extra.is_enabled == Some(true) {
                if let (Some(used_credits), Some(monthly_limit)) =
                    (extra.used_credits, extra.monthly_limit)
                {
                    quotas.push(QuotaInfo::with_details(
                        QuotaLabelSpec::ExtraUsage,
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
