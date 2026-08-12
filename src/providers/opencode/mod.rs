mod auth;
mod client;
mod parser;

use super::{
    AiProvider, ProviderCapabilities, ProviderError, ProviderExecutionContext, ProviderResult,
};
use crate::models::{
    FailureAdvice, ProviderDescriptor, ProviderKind, ProviderMetadata, RefreshData,
};
use crate::providers::common::http_client::HttpError;
use async_trait::async_trait;
use log::debug;
use std::borrow::Cow;

use auth::load_auth;
use client::fetch_usage;
use parser::parse_usage_response;

super::define_unit_provider!(OpenCodeProvider);

#[async_trait]
impl AiProvider for OpenCodeProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: Cow::Borrowed("opencode:api"),
            metadata: ProviderMetadata {
                kind: ProviderKind::OpenCode,
                display_name: "OpenCode Go".into(),
                brand_name: "OpenCode Go".into(),
                icon_asset: "src/icons/provider-opencode.svg".into(),
                dashboard_url: "https://opencode.ai".into(),
                account_hint: "OpenCode Go account".into(),
                source_label: "opencode api".into(),
            },
        }
    }

    async fn check_availability(&self, _ctx: &ProviderExecutionContext<'_>) -> ProviderResult<()> {
        let auth = load_auth()?;
        debug!(
            target: "providers",
            "OpenCode availability: ok (auth provider id: {})",
            auth.provider_id
        );
        Ok(())
    }

    async fn refresh(&self, _ctx: &ProviderExecutionContext<'_>) -> ProviderResult<RefreshData> {
        let auth = load_auth()?;
        debug!(
            target: "providers",
            "opencode: fetching Go usage from {} (auth provider id: {})",
            client::USAGE_URL,
            auth.provider_id
        );

        let body = match fetch_usage(&auth.api_key) {
            Ok(body) => body,
            Err(err) => {
                if let Some(http_err) = err.downcast_ref::<HttpError>() {
                    match http_err {
                        HttpError::HttpStatus { code: 401 } => {
                            return Err(ProviderError::session_expired(Some(
                                FailureAdvice::LoginApp {
                                    app: "OpenCode Go".to_string(),
                                },
                            )));
                        }
                        HttpError::HttpStatus { code: 403 } => {
                            // 官方对「无 Go 订阅」也返回 403 EntitlementError。
                            return Err(ProviderError::auth_required(Some(
                                FailureAdvice::ApiError {
                                    message: "OpenCode Go subscription required, or the API key is invalid. Connect OpenCode Go in the OpenCode TUI and ensure auth.json has an opencode-go / opencode API key.".to_string(),
                                },
                            )));
                        }
                        _ => {}
                    }
                }
                return Err(err.into());
            }
        };

        Ok(RefreshData::quotas_only(parse_usage_response(&body)?))
    }
}

impl ProviderCapabilities for OpenCodeProvider {}
