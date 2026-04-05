mod client;
mod parser;
pub mod settings_ui;
mod token;

use super::{AiProvider, ProviderError};
use crate::models::{ProviderDescriptor, ProviderKind, ProviderMetadata, RefreshData};
use anyhow::{Context, Result};
use async_trait::async_trait;
use log::debug;
use std::borrow::Cow;

use client::{fetch_github_user, fetch_user_info};
use parser::{parse_github_user, parse_user_info_response};
#[allow(unused_imports)]
pub use token::{resolve_token, CopilotTokenSource, CopilotTokenStatus};

super::define_unit_provider!(CopilotProvider);

#[async_trait]
impl AiProvider for CopilotProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: Cow::Borrowed("copilot:api"),
            metadata: ProviderMetadata {
                kind: ProviderKind::Copilot,
                display_name: "Copilot".into(),
                brand_name: "GitHub".into(),
                icon_asset: "src/icons/provider-copilot.svg".into(),
                dashboard_url: "https://github.com/settings/copilot".into(),
                account_hint: "GitHub account".into(),
                source_label: "github api".into(),
            },
        }
    }

    async fn check_availability(&self) -> Result<()> {
        let token_status = resolve_token(None);
        let available = token_status.token.is_some();
        debug!(
            target: "providers",
            "Copilot availability: {} (token source: {})",
            available,
            token_status.source.log_label()
        );
        if available {
            Ok(())
        } else {
            Err(ProviderError::config_missing("github_token / GITHUB_TOKEN").into())
        }
    }

    async fn refresh(&self) -> Result<RefreshData> {
        let token_status = resolve_token(None);

        let token = token_status.token.context(
            "GitHub token not configured. Set github_token in settings, or GITHUB_TOKEN environment variable.",
        )?;

        // 并行获取 Copilot 配额和 GitHub 用户信息
        let (body, status_code) = fetch_user_info(&token)?;

        // /user API 获取账户标识（best-effort，失败不影响配额数据）
        let account_name = fetch_github_user(&token)
            .ok()
            .and_then(|(user_body, _)| parse_github_user(&user_body));

        parse_user_info_response(&body, &status_code, account_name)
    }
}
