mod auth;
mod client;
mod parser;

use super::{
    AiProvider, ProviderCapabilities, ProviderError, ProviderExecutionContext, ProviderResult,
};
use crate::models::{ProviderDescriptor, ProviderKind, ProviderMetadata, RefreshData};
use crate::providers::common::http_client::HttpError;
use async_trait::async_trait;
use std::borrow::Cow;

use auth::{
    auth_path, ensure_access_token, load_credentials, refresh_and_reload, session_expired,
    GrokCredentials,
};
use client::fetch_billing;
use parser::parse_usage_response;

super::define_unit_provider!(GrokProvider);

#[async_trait]
impl AiProvider for GrokProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: Cow::Borrowed("grok:api"),
            metadata: ProviderMetadata {
                kind: ProviderKind::Grok,
                display_name: "Grok".into(),
                brand_name: "xAI".into(),
                icon_asset: "src/icons/provider-grok.svg".into(),
                dashboard_url: "https://grok.com?_s=usage".into(),
                account_hint: "Grok account".into(),
                source_label: "grok api".into(),
            },
        }
    }

    async fn check_availability(&self, _ctx: &ProviderExecutionContext<'_>) -> ProviderResult<()> {
        if auth_path().exists() {
            Ok(())
        } else {
            Err(ProviderError::config_missing("~/.grok/auth.json"))
        }
    }

    async fn refresh(&self, _ctx: &ProviderExecutionContext<'_>) -> ProviderResult<RefreshData> {
        let mut credentials = load_credentials()?;
        ensure_access_token(&mut credentials);
        let body = fetch_billing_authed(&mut credentials)?;
        Ok(RefreshData::with_account(
            parse_usage_response(&body)?,
            credentials.email,
            None,
        ))
    }
}

impl ProviderCapabilities for GrokProvider {}

/// 先用当前 token 拉 billing；401/403 时刷新一次再重试。
fn fetch_billing_authed(credentials: &mut GrokCredentials) -> ProviderResult<String> {
    match fetch_billing(&credentials.access_token) {
        Ok(body) => Ok(body),
        Err(err) if is_auth_error(&err) => {
            refresh_and_reload(credentials).map_err(|_| session_expired())?;
            fetch_billing(&credentials.access_token).map_err(map_billing_error)
        }
        Err(err) => Err(err.into()),
    }
}

fn map_billing_error(err: anyhow::Error) -> ProviderError {
    if is_auth_error(&err) {
        session_expired()
    } else {
        err.into()
    }
}

fn is_auth_error(err: &anyhow::Error) -> bool {
    err.downcast_ref::<HttpError>()
        .is_some_and(HttpError::is_auth_error)
}
