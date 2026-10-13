mod auth;
mod client;
mod parser;

use auth::{resolve_token, GatewayPin, TokenSource, ZCodeCredential};
use client::{fetch_quota, CHINA_QUOTA_URL, GLOBAL_QUOTA_URL};
use parser::{parse_quota_response, QuotaSnapshot};

use super::{
    AiProvider, ProviderCapabilities, ProviderError, ProviderExecutionContext, ProviderResult,
};
use crate::models::{
    AppSettings, ProviderDescriptor, ProviderKind, ProviderMetadata, RefreshData,
    SettingsCapability, TokenEditMode, TokenInputCapability, TokenInputState,
};
use crate::providers::common::http_client::HttpError;
use async_trait::async_trait;
use log::debug;
use std::borrow::Cow;

super::define_unit_provider!(ZCodeProvider);

fn zcode_settings_capability() -> SettingsCapability {
    SettingsCapability::TokenInput(TokenInputCapability {
        credential_key: "zcode_api_key",
        placeholder_i18n_key: "zcode.token_placeholder",
        help_tip_i18n_key: "zcode.token_sources_tip",
        title_i18n_key: "zcode.api_key",
        description_i18n_key: "zcode.requires_auth",
        create_url: "https://z.ai/manage-apikey/apikey-list",
    })
}

#[async_trait]
impl AiProvider for ZCodeProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: Cow::Borrowed("zcode:api"),
            metadata: ProviderMetadata {
                kind: ProviderKind::ZCode,
                display_name: "ZCode".into(),
                brand_name: "Z.ai".into(),
                icon_asset: "src/icons/provider-zcode.svg".into(),
                dashboard_url: "https://z.ai/subscription".into(),
                account_hint: "GLM Coding Plan".into(),
                source_label: "zcode api".into(),
            },
        }
    }

    async fn check_availability(&self, ctx: &ProviderExecutionContext<'_>) -> ProviderResult<()> {
        let credential = resolve_token(ctx.provider_credentials.get_credential("zcode_api_key"))?;
        debug!(
            target: "providers",
            "ZCode availability: ok (token source: {}, gateway: {})",
            credential.source.log_label(),
            gateway_log_label(credential.gateway)
        );
        Ok(())
    }

    async fn refresh(&self, ctx: &ProviderExecutionContext<'_>) -> ProviderResult<RefreshData> {
        let credential = resolve_token(ctx.provider_credentials.get_credential("zcode_api_key"))?;
        debug!(
            target: "providers",
            "zcode: fetching coding-plan quota (token source: {}, gateway: {})",
            credential.source.log_label(),
            gateway_log_label(credential.gateway)
        );
        let snapshot = load_snapshot(credential.gateway, |url| {
            fetch_quota(url, &credential.token)
        })?;
        Ok(RefreshData::with_account(
            snapshot.quotas,
            None,
            snapshot.tier,
        ))
    }
}

impl ProviderCapabilities for ZCodeProvider {
    fn settings_capability(&self) -> SettingsCapability {
        zcode_settings_capability()
    }

    fn resolve_token_input_state(&self, settings: &AppSettings) -> Option<TokenInputState> {
        let SettingsCapability::TokenInput(config) = self.settings_capability() else {
            return None;
        };
        let status = resolve_token(
            settings
                .provider
                .credentials
                .get_credential(config.credential_key),
        )
        .ok();
        let source_i18n_key = status.as_ref().map(|status| match status.source {
            TokenSource::ConfigFile => "zcode.source.config_file",
            TokenSource::EnvVar => "zcode.source.env_var",
            TokenSource::LocalApiKey => "zcode.source.local_api_key",
        });
        let has_token = status.is_some();
        Some(TokenInputState {
            has_token,
            masked: status.as_ref().map(ZCodeCredential::masked),
            source_i18n_key,
            edit_mode: if matches!(
                status.as_ref().map(|status| status.source),
                Some(TokenSource::ConfigFile)
            ) && has_token
            {
                TokenEditMode::EditStored
            } else {
                TokenEditMode::SetNew
            },
        })
    }
}

fn gateway_log_label(gateway: GatewayPin) -> &'static str {
    match gateway {
        GatewayPin::Global => "global",
        GatewayPin::China => "china",
        GatewayPin::Auto => "auto",
    }
}

fn quota_urls(gateway: GatewayPin) -> &'static [&'static str] {
    match gateway {
        GatewayPin::Global => &[GLOBAL_QUOTA_URL],
        GatewayPin::China => &[CHINA_QUOTA_URL],
        GatewayPin::Auto => &[GLOBAL_QUOTA_URL, CHINA_QUOTA_URL],
    }
}

/// 未指定网关时先问国际站。认证失败或网络失败再试国内站。
/// 第二次仍连不上时保留第一次的错误，避免把「国际站超时」说成国内站的失败。
fn load_snapshot<F>(gateway: GatewayPin, mut fetch: F) -> ProviderResult<QuotaSnapshot>
where
    F: FnMut(&str) -> anyhow::Result<String>,
{
    let urls = quota_urls(gateway);
    let mut first_transport = None;
    for (index, url) in urls.iter().enumerate() {
        let last = index + 1 == urls.len();
        match fetch(url) {
            Ok(body) => match parse_quota_response(&body) {
                Ok(snapshot) => return Ok(snapshot),
                Err(err) if is_auth_error(&err) && !last => {
                    debug!(
                        target: "providers",
                        "zcode: quota rejected on {url}, trying the other gateway"
                    );
                }
                Err(err) => return Err(err),
            },
            Err(err) if is_auth_http(&err) && !last => {
                debug!(
                    target: "providers",
                    "zcode: quota unauthorized on {url}, trying the other gateway"
                );
            }
            Err(err) if is_transport(&err) && !last => {
                debug!(
                    target: "providers",
                    "zcode: quota request failed on {url}, trying the other gateway"
                );
                first_transport.get_or_insert(ProviderError::classify(&err));
            }
            Err(err) if is_transport(&err) => {
                return Err(first_transport.unwrap_or_else(|| ProviderError::classify(&err)));
            }
            Err(err) => return Err(ProviderError::classify(&err)),
        }
    }
    Err(first_transport.unwrap_or_else(|| {
        ProviderError::auth_required(Some(crate::models::FailureAdvice::ApiHttpError {
            status: "401".to_string(),
        }))
    }))
}

fn is_auth_error(err: &ProviderError) -> bool {
    matches!(err, ProviderError::AuthRequired { .. })
}

fn is_auth_http(err: &anyhow::Error) -> bool {
    err.downcast_ref::<HttpError>()
        .is_some_and(HttpError::is_auth_error)
}

fn is_transport(err: &anyhow::Error) -> bool {
    matches!(
        err.downcast_ref::<HttpError>(),
        Some(HttpError::Timeout | HttpError::Transport(_))
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::QuotaLabelSpec;

    const CREDIT_BODY: &str = r#"{
        "code": 200,
        "data": {
            "level": "pro",
            "limits": [
                {"type": "CREDIT_LIMIT", "unit": 3, "number": 5, "percentage": 10, "nextResetTime": 1786073946574},
                {"type": "CREDIT_LIMIT", "unit": 6, "number": 1, "percentage": 20, "nextResetTime": 1786660486998}
            ]
        }
    }"#;

    #[test]
    fn auto_gateway_retries_china_after_global_auth_failure() {
        let mut calls = Vec::new();
        let snapshot = load_snapshot(GatewayPin::Auto, |url| {
            calls.push(url.to_string());
            if url == GLOBAL_QUOTA_URL {
                Err(HttpError::HttpStatus { code: 401 }.into())
            } else {
                Ok(CREDIT_BODY.to_string())
            }
        })
        .unwrap();
        assert_eq!(
            calls,
            vec![GLOBAL_QUOTA_URL.to_string(), CHINA_QUOTA_URL.to_string()]
        );
        assert_eq!(snapshot.tier.as_deref(), Some("Pro"));
        assert_eq!(snapshot.quotas[0].label_spec, QuotaLabelSpec::Session);
        assert_eq!(snapshot.quotas[1].label_spec, QuotaLabelSpec::Weekly);
    }

    #[test]
    fn auto_gateway_retries_when_the_body_says_unauthorized() {
        let mut calls = 0;
        let snapshot = load_snapshot(GatewayPin::Auto, |_| {
            calls += 1;
            if calls == 1 {
                Ok(r#"{"code":401,"success":false}"#.to_string())
            } else {
                Ok(CREDIT_BODY.to_string())
            }
        })
        .unwrap();
        assert_eq!(calls, 2);
        assert_eq!(snapshot.quotas.len(), 2);
    }

    #[test]
    fn auto_gateway_retries_china_after_global_timeout() {
        let snapshot = load_snapshot(GatewayPin::Auto, |url| {
            if url == GLOBAL_QUOTA_URL {
                Err(HttpError::Timeout.into())
            } else {
                Ok(CREDIT_BODY.to_string())
            }
        })
        .unwrap();
        assert_eq!(snapshot.quotas.len(), 2);
        assert_eq!(snapshot.quotas[0].label_spec, QuotaLabelSpec::Session);
    }

    #[test]
    fn pinned_gateway_does_not_retry() {
        let mut calls = 0;
        let err = load_snapshot(GatewayPin::Global, |url| {
            calls += 1;
            assert_eq!(url, GLOBAL_QUOTA_URL);
            Err(HttpError::HttpStatus { code: 401 }.into())
        })
        .unwrap_err();
        assert_eq!(calls, 1);
        assert!(matches!(err, ProviderError::AuthRequired { .. }));
    }

    #[test]
    fn http_failure_does_not_fall_through_to_the_other_gateway() {
        let mut calls = 0;
        let err = load_snapshot(GatewayPin::Auto, |_| {
            calls += 1;
            Err(HttpError::HttpStatus { code: 500 }.into())
        })
        .unwrap_err();
        assert_eq!(calls, 1);
        assert!(matches!(err, ProviderError::FetchFailed { .. }));
    }

    #[test]
    fn transport_failure_on_both_gateways_keeps_the_first_error() {
        let err = load_snapshot(GatewayPin::Auto, |url| {
            if url == GLOBAL_QUOTA_URL {
                Err(HttpError::Timeout.into())
            } else {
                Err(HttpError::Transport("china down".to_string()).into())
            }
        })
        .unwrap_err();
        assert!(matches!(err, ProviderError::Timeout));
    }
}
