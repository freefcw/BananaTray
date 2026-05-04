use crate::models::{QuotaDetailSpec, QuotaInfo, QuotaType};
use crate::providers::{ProviderError, ProviderResult};
use crate::utils::time_utils::parse_iso8601_to_epoch;
use prost::Message;

/// 解析策略：针对不同载荷格式提取同一组领域数据。
///
/// 这里解决的是“同一来源的不同编码格式如何解析”，不是“多个来源之间如何回退”。
/// 因此它与 Claude 的 `UsageProbe` 处于不同抽象层级，不应硬统一。
pub trait ParseStrategy {
    /// Parse user status from raw data
    /// Returns (quotas, email, plan_name)
    fn parse(
        &self,
        data: &[u8],
    ) -> ProviderResult<(Vec<QuotaInfo>, Option<String>, Option<String>)>;
}

/// Parse strategy for Codeium-family API JSON response
pub struct ApiParseStrategy;

impl ParseStrategy for ApiParseStrategy {
    fn parse(
        &self,
        data: &[u8],
    ) -> ProviderResult<(Vec<QuotaInfo>, Option<String>, Option<String>)> {
        let json: serde_json::Value = serde_json::from_slice(data)
            .map_err(|_| ProviderError::parse_failed("Codeium-family API response"))?;

        let user_status = json
            .get("userStatus")
            .ok_or_else(|| ProviderError::parse_failed("missing 'userStatus' field"))?;

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

                    let reset_detail = quota_info
                        .get("resetTime")
                        .and_then(|v| v.as_str())
                        .and_then(parse_iso8601_to_epoch)
                        .map(|epoch_secs| QuotaDetailSpec::ResetAt { epoch_secs });

                    quotas.push(QuotaInfo::with_details(
                        label,
                        used_percent,
                        100.0,
                        QuotaType::ModelSpecific(label.to_string()),
                        reset_detail,
                    ));
                }
            }
        }

        quotas.sort_by(|a, b| a.stable_key.cmp(&b.stable_key));

        if quotas.is_empty() {
            return Err(ProviderError::no_data());
        }

        Ok((quotas, email, plan_name))
    }
}

/// Parse strategy for local cache protobuf data
pub struct CacheParseStrategy;

impl ParseStrategy for CacheParseStrategy {
    fn parse(
        &self,
        data: &[u8],
    ) -> ProviderResult<(Vec<QuotaInfo>, Option<String>, Option<String>)> {
        let user_status = ProtoUserStatus::decode(data)
            .map_err(|_| ProviderError::parse_failed("Codeium-family cache protobuf"))?;

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
            let now_ts = chrono::Utc::now().timestamp();
            for model_config in model_configs.configs {
                let label = model_config.label.clone();

                if let Some(quota_info) = model_config.quota_info {
                    let remaining_fraction = quota_info.remaining_fraction.unwrap_or(0.0);
                    let reset_at = quota_info
                        .reset_time_wrapper
                        .and_then(|wrapper| wrapper.reset_time);

                    // reset 时间已过 → 服务端已重置配额，缓存中的 remaining_fraction
                    // 是过期数据；视为 100% 剩余，且不再显示倒计时（与 cached_plan 路径一致）。
                    let is_stale = reset_at.is_some_and(|ts| ts <= now_ts);
                    let effective_remaining = if is_stale { 1.0 } else { remaining_fraction };
                    let used_percent = (1.0 - effective_remaining) * 100.0;
                    let reset_detail = if is_stale {
                        None
                    } else {
                        reset_at.map(|epoch_secs| QuotaDetailSpec::ResetAt { epoch_secs })
                    };

                    quotas.push(QuotaInfo::with_details(
                        label.clone(),
                        used_percent as f64,
                        100.0,
                        QuotaType::ModelSpecific(label),
                        reset_detail,
                    ));
                }
            }
        }

        quotas.sort_by(|a, b| a.stable_key.cmp(&b.stable_key));

        if quotas.is_empty() {
            return Err(ProviderError::no_data());
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

    fn build_proto_payload(remaining: f32, reset_time: i64) -> Vec<u8> {
        let user_status = ProtoUserStatus {
            version: 1,
            display_name: "alice".into(),
            email: "alice@example.com".into(),
            model_configs: Some(ProtoModelConfigs {
                configs: vec![ProtoModelConfig {
                    label: "model-x".into(),
                    quota_info: Some(ProtoQuotaInfo {
                        remaining_fraction: Some(remaining),
                        reset_time_wrapper: Some(ProtoResetTime {
                            reset_time: Some(reset_time),
                        }),
                    }),
                }],
            }),
            tier: Some(ProtoTierInfo {
                id: "pro".into(),
                name: "Pro".into(),
            }),
        };
        user_status.encode_to_vec()
    }

    #[test]
    fn test_cache_parse_strategy_fresh_reset_keeps_remaining() {
        // reset 在未来 → 使用缓存中的 remaining_fraction
        let future = chrono::Utc::now().timestamp() + 3600;
        let bytes = build_proto_payload(0.4, future);

        let (quotas, _, _) = CacheParseStrategy.parse(&bytes).unwrap();
        assert_eq!(quotas.len(), 1);
        assert!((quotas[0].used - 60.0).abs() < 0.01); // 1 - 0.4 = 60% used
        assert!(matches!(
            quotas[0].detail_spec,
            Some(QuotaDetailSpec::ResetAt { .. })
        ));
    }

    #[test]
    fn test_cache_parse_strategy_stale_reset_treated_as_full() {
        // reset 时间已过 → 服务端已重置，缓存的 0.4 是陈旧数据，应视为 100% 剩余
        let past = chrono::Utc::now().timestamp() - 3600;
        let bytes = build_proto_payload(0.4, past);

        let (quotas, _, _) = CacheParseStrategy.parse(&bytes).unwrap();
        assert_eq!(quotas.len(), 1);
        assert!(quotas[0].used.abs() < 0.01); // 0% used = 100% remaining
        assert!(quotas[0].detail_spec.is_none()); // 不再展示已过期的倒计时
    }
}
