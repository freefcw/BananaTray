use crate::models::{QuotaInfo, QuotaType};
use crate::utils::time_utils;
use anyhow::Result;
use prost::Message;

/// 解析策略：针对不同载荷格式提取同一组领域数据。
///
/// 这里解决的是“同一来源的不同编码格式如何解析”，不是“多个来源之间如何回退”。
/// 因此它与 Claude 的 `UsageProbe` 处于不同抽象层级，不应硬统一。
pub trait ParseStrategy {
    /// Parse user status from raw data
    /// Returns (quotas, email, plan_name)
    fn parse(&self, data: &[u8]) -> Result<(Vec<QuotaInfo>, Option<String>, Option<String>)>;
}

/// Parse strategy for Codeium-family API JSON response
pub struct ApiParseStrategy;

impl ParseStrategy for ApiParseStrategy {
    fn parse(&self, data: &[u8]) -> Result<(Vec<QuotaInfo>, Option<String>, Option<String>)> {
        let json: serde_json::Value = serde_json::from_slice(data)?;

        let user_status = json
            .get("userStatus")
            .ok_or_else(|| anyhow::anyhow!("missing 'userStatus' field"))?;

        let email = user_status
            .get("email")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        let plan_name = user_status
            .pointer("/userTier/name")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .or_else(|| {
                user_status
                    .pointer("/planStatus/planInfo/planName")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
            })
            .map(|s| s.to_string());

        let mut quotas = Vec::new();

        let model_configs = user_status
            .pointer("/cascadeModelConfigData/clientModelConfigs")
            .and_then(|v| v.as_array());

        if let Some(configs) = model_configs {
            for config in configs {
                let label = config
                    .get("label")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");

                if let Some(quota_info) = config.get("quotaInfo") {
                    let fraction = quota_info
                        .get("remainingFraction")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    let used_percent = (1.0 - fraction) * 100.0;

                    let reset_text = quota_info
                        .get("resetTime")
                        .and_then(|v| v.as_str())
                        .and_then(time_utils::format_reset_countdown);

                    quotas.push(QuotaInfo::with_details(
                        label,
                        used_percent,
                        100.0,
                        QuotaType::ModelSpecific(label.to_string()),
                        reset_text,
                    ));
                }
            }
        }

        quotas.sort_by(|a, b| a.label.cmp(&b.label));

        if quotas.is_empty() {
            anyhow::bail!("no model quotas found in API response");
        }

        Ok((quotas, email, plan_name))
    }
}

/// Parse strategy for local cache protobuf data
pub struct CacheParseStrategy;

impl ParseStrategy for CacheParseStrategy {
    fn parse(&self, data: &[u8]) -> Result<(Vec<QuotaInfo>, Option<String>, Option<String>)> {
        let user_status = ProtoUserStatus::decode(data)?;

        let email = if user_status.email.is_empty() {
            None
        } else {
            Some(user_status.email.clone())
        };

        let plan_name = user_status.tier.and_then(|t| {
            if t.name.is_empty() {
                None
            } else {
                Some(t.name)
            }
        });

        let mut quotas = Vec::new();

        if let Some(model_configs) = user_status.model_configs {
            for model_config in model_configs.configs {
                let label = model_config.label.clone();

                if let Some(quota_info) = model_config.quota_info {
                    let remaining_fraction = quota_info.remaining_fraction.unwrap_or(0.0);
                    let used_percent = (1.0 - remaining_fraction) * 100.0;

                    let reset_text = quota_info
                        .reset_time_wrapper
                        .and_then(|wrapper| wrapper.reset_time)
                        .and_then(|ts| {
                            chrono::DateTime::from_timestamp(ts, 0)
                                .map(|dt| dt.format("%Y-%m-%dT%H:%M:%SZ").to_string())
                        })
                        .as_deref()
                        .and_then(time_utils::format_reset_countdown);

                    quotas.push(QuotaInfo::with_details(
                        label.clone(),
                        used_percent as f64,
                        100.0,
                        QuotaType::ModelSpecific(label),
                        reset_text,
                    ));
                }
            }
        }

        quotas.sort_by(|a, b| a.label.cmp(&b.label));

        if quotas.is_empty() {
            anyhow::bail!("no model quotas found in cache");
        }

        Ok((quotas, email, plan_name))
    }
}

#[derive(Clone, PartialEq, Message)]
pub struct ProtoUserStatus {
    #[prost(uint32, tag = "2")]
    pub version: u32,

    #[prost(string, tag = "3")]
    pub display_name: String,

    #[prost(string, tag = "7")]
    pub email: String,

    #[prost(message, optional, tag = "33")]
    pub model_configs: Option<ProtoModelConfigs>,

    #[prost(message, optional, tag = "36")]
    pub tier: Option<ProtoTierInfo>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ProtoModelConfigs {
    #[prost(message, repeated, tag = "1")]
    pub configs: Vec<ProtoModelConfig>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ProtoModelConfig {
    #[prost(string, tag = "1")]
    pub label: String,

    #[prost(message, optional, tag = "15")]
    pub quota_info: Option<ProtoQuotaInfo>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ProtoQuotaInfo {
    #[prost(float, optional, tag = "1")]
    pub remaining_fraction: Option<f32>,

    #[prost(message, optional, tag = "2")]
    pub reset_time_wrapper: Option<ProtoResetTime>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ProtoResetTime {
    #[prost(int64, optional, tag = "1")]
    pub reset_time: Option<i64>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ProtoTierInfo {
    #[prost(string, tag = "1")]
    pub id: String,

    #[prost(string, tag = "2")]
    pub name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_parse_strategy() {
        let json = r#"{
            "userStatus": {
                "email": "test@example.com",
                "userTier": { "name": "Pro" },
                "cascadeModelConfigData": {
                    "clientModelConfigs": [
                        {
                            "label": "model-a",
                            "quotaInfo": { "remainingFraction": 0.75 }
                        }
                    ]
                }
            }
        }"#;

        let strategy = ApiParseStrategy;
        let (quotas, email, plan) = strategy.parse(json.as_bytes()).unwrap();

        assert_eq!(email, Some("test@example.com".to_string()));
        assert_eq!(plan, Some("Pro".to_string()));
        assert_eq!(quotas.len(), 1);
        assert!((quotas[0].used - 25.0).abs() < 0.01);
    }
}
