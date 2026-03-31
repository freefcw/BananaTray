use super::{AiProvider, ProviderError};
use crate::models::{ProviderKind, ProviderMetadata, QuotaInfo, QuotaType};
use crate::utils::http_client;
use crate::utils::time_utils;
use anyhow::{Context, Result};
use async_trait::async_trait;
use rust_i18n::t;
use std::path::PathBuf;
use std::process::Command;

super::define_unit_provider!(CursorProvider);

impl CursorProvider {
    /// Path to Cursor's SQLite database.
    fn db_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Library/Application Support/Cursor/User/globalStorage/state.vscdb")
    }

    /// Read the access token from Cursor's SQLite database via `sqlite3` CLI.
    fn read_access_token() -> Result<String> {
        let db_path = Self::db_path();
        let db_str = db_path.to_string_lossy();

        let output = Command::new("sqlite3")
            .args([
                &*db_str,
                "SELECT value FROM ItemTable WHERE key = 'cursorAuth/accessToken'",
            ])
            .output()
            .map_err(|_| ProviderError::cli_not_found("sqlite3"))?;

        if !output.status.success() {
            return Err(ProviderError::fetch_failed(&format!(
                "sqlite3 exit code {:?}",
                output.status.code()
            ))
            .into());
        }

        let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if token.is_empty() {
            return Err(
                ProviderError::auth_required(Some(&t!("hint.login_app", app = "Cursor"))).into(),
            );
        }

        Ok(token)
    }

    /// Decode the JWT payload (second segment) and extract the `sub` field as userId.
    fn extract_user_id_from_jwt(token: &str) -> Result<String> {
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() < 2 {
            return Err(ProviderError::parse_failed("invalid JWT format").into());
        }

        // Base64url decode the payload
        let mut b64 = parts[1].replace('-', "+").replace('_', "/");
        let remainder = b64.len() % 4;
        if remainder > 0 {
            b64.push_str(&"=".repeat(4 - remainder));
        }

        use base64::Engine;
        let payload_bytes = base64::engine::general_purpose::STANDARD
            .decode(&b64)
            .map_err(|_| ProviderError::parse_failed("JWT payload Base64 decode failed"))?;

        let payload: serde_json::Value = serde_json::from_slice(&payload_bytes)
            .map_err(|_| ProviderError::parse_failed("JWT payload JSON parse failed"))?;

        let sub = payload
            .get("sub")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ProviderError::parse_failed("JWT missing 'sub' field"))?;

        Ok(sub.to_string())
    }

    /// Fetch usage summary from Cursor API.
    fn fetch_usage_summary(cookie: &str) -> Result<String> {
        let cookie_header = format!("Cookie: {}", cookie);
        http_client::get(
            "https://cursor.com/api/usage-summary",
            &[&cookie_header, "Content-Type: application/json"],
        )
    }

    /// Parse the usage-summary JSON response into QuotaInfo entries.
    fn parse_usage_response(body: &str) -> Result<Vec<QuotaInfo>> {
        let json: serde_json::Value = serde_json::from_str(body)
            .map_err(|_| ProviderError::parse_failed("usage-summary response"))?;

        let mut quotas = Vec::new();

        let membership_type = json
            .get("membershipType")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let is_unlimited = json
            .get("isUnlimited")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Parse reset time from billingCycleEnd
        let reset_at = json
            .get("billingCycleEnd")
            .and_then(|v| v.as_str())
            .and_then(time_utils::format_reset_countdown);

        let tier_label = membership_type.to_uppercase();

        // Handle unlimited plans
        if is_unlimited {
            quotas.push(QuotaInfo::with_details(
                t!("quota.label.monthly_tier", tier = tier_label).to_string(),
                0.0,
                1.0,
                QuotaType::General,
                Some(t!("quota.label.unlimited").to_string()),
            ));
            return Ok(quotas);
        }

        let individual_usage = json.get("individualUsage");
        let limit_type = json.get("limitType").and_then(|v| v.as_str()).unwrap_or("");

        // Parse plan usage (included requests)
        if let Some(plan) = individual_usage.and_then(|u| u.get("plan")) {
            let enabled = plan
                .get("enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if enabled {
                let used = plan.get("used").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let limit = plan.get("limit").and_then(|v| v.as_f64()).unwrap_or(0.0);

                // Enterprise plans may have limit == 0; fall back to breakdown.total
                let breakdown_total = plan
                    .get("breakdown")
                    .and_then(|b| b.get("total"))
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let effective_limit = if limit > 0.0 { limit } else { breakdown_total };

                if effective_limit > 0.0 {
                    // When limit is 0, derive used from totalPercentUsed (enterprise quirk)
                    let effective_used = if limit == 0.0 {
                        plan.get("totalPercentUsed")
                            .and_then(|v| v.as_f64())
                            .map(|pct| (pct * effective_limit / 100.0).round())
                            .unwrap_or(used)
                    } else {
                        used
                    };

                    quotas.push(QuotaInfo::with_details(
                        t!("quota.label.monthly_tier", tier = tier_label).to_string(),
                        effective_used,
                        effective_limit,
                        QuotaType::General,
                        reset_at.clone(),
                    ));
                }
            }
        }

        // Parse on-demand usage
        if let Some(on_demand) = individual_usage.and_then(|u| u.get("onDemand")) {
            let enabled = on_demand
                .get("enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if enabled {
                let used = on_demand
                    .get("used")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let limit = on_demand
                    .get("limit")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                if limit > 0.0 {
                    quotas.push(QuotaInfo::with_details(
                        t!("quota.label.on_demand").to_string(),
                        used,
                        limit,
                        QuotaType::Credit,
                        reset_at.clone(),
                    ));
                }
            }
        }

        // Parse team usage for enterprise plans (limitType == "team")
        if limit_type == "team" {
            if let Some(team_on_demand) = json.get("teamUsage").and_then(|t| t.get("onDemand")) {
                let enabled = team_on_demand
                    .get("enabled")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if enabled {
                    let used = team_on_demand
                        .get("used")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    let limit = team_on_demand
                        .get("limit")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    if limit > 0.0 {
                        quotas.push(QuotaInfo::with_details(
                            t!("quota.label.team").to_string(),
                            used,
                            limit,
                            QuotaType::Credit,
                            reset_at.clone(),
                        ));
                    }
                }
            }
        }

        if quotas.is_empty() {
            return Err(ProviderError::no_data().into());
        }

        Ok(quotas)
    }
}

#[async_trait]
impl AiProvider for CursorProvider {
    fn metadata(&self) -> ProviderMetadata {
        ProviderMetadata {
            kind: ProviderKind::Cursor,
            display_name: "Cursor".into(),
            brand_name: "Cursor".into(),
            icon_asset: "src/icons/provider-cursor.svg".into(),
            dashboard_url: "https://cursor.com/dashboard?tab=usage".into(),
            account_hint: "Cursor account".into(),
            source_label: "cursor api".into(),
        }
    }

    fn id(&self) -> &'static str {
        "cursor:api"
    }

    async fn is_available(&self) -> bool {
        Self::db_path().exists()
    }

    async fn refresh_quotas(&self) -> Result<Vec<QuotaInfo>> {
        let access_token =
            Self::read_access_token().context("Failed to read Cursor access token")?;

        let user_id = Self::extract_user_id_from_jwt(&access_token)
            .context("Failed to extract user ID from Cursor JWT")?;

        let cookie = format!("WorkosCursorSessionToken={}::{}", user_id, access_token);

        let body =
            Self::fetch_usage_summary(&cookie).context("Failed to fetch Cursor usage summary")?;

        Self::parse_usage_response(&body)
    }
}
