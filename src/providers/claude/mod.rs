//! Claude Provider
//!
//! 支持 CLI 和 OAuth API 两种获取方式，自动选择最优方式。

mod api_probe;
mod cli_probe;
mod credentials;
mod probe;

pub use probe::{ProbeMode, UsageProbe};

use api_probe::ClaudeApiProbe;
use async_trait::async_trait;
use cli_probe::ClaudeCliProbe;
use log::debug;

use super::{AiProvider, ProviderError};
use crate::models::{ProviderKind, ProviderMetadata, QuotaInfo};
use anyhow::Result;

/// Claude Provider
///
/// 支持三种获取模式：
/// - Auto: 优先 API，失败回退 CLI（默认）
/// - Cli: 强制使用 CLI
/// - Api: 强制使用 API
pub struct ClaudeProvider {
    cli_probe: ClaudeCliProbe,
    api_probe: ClaudeApiProbe,
    probe_mode: ProbeMode,
}

impl Default for ClaudeProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ClaudeProvider {
    pub fn new() -> Self {
        Self {
            cli_probe: ClaudeCliProbe,
            api_probe: ClaudeApiProbe,
            probe_mode: ProbeMode::Auto,
        }
    }

    /// 读取账户邮箱（从 ~/.claude.json）
    #[allow(dead_code)]
    pub fn read_account_email() -> Option<String> {
        let home = dirs::home_dir()?;
        let path = home.join(".claude.json");
        let content = std::fs::read_to_string(path).ok()?;
        let json: serde_json::Value = serde_json::from_str(&content).ok()?;
        json.get("oauthAccount")
            .and_then(|a| a.get("emailAddress"))
            .and_then(|e| e.as_str())
            .map(|s| s.to_string())
    }
}

#[async_trait]
impl AiProvider for ClaudeProvider {
    fn metadata(&self) -> ProviderMetadata {
        ProviderMetadata {
            kind: ProviderKind::Claude,
            display_name: "Claude".into(),
            brand_name: "Anthropic".into(),
            icon_asset: "src/icons/provider-claude.svg".into(),
            dashboard_url: "https://console.anthropic.com/settings/billing".into(),
            account_hint: "Anthropic workspace".into(),
            source_label: "claude".into(),
        }
    }

    fn id(&self) -> &'static str {
        "claude"
    }

    async fn is_available(&self) -> bool {
        self.cli_probe.is_available() || self.api_probe.is_available()
    }

    async fn refresh_quotas(&self) -> Result<Vec<QuotaInfo>> {
        match self.probe_mode {
            ProbeMode::Cli => {
                debug!("Claude: 强制使用 CLI 模式");
                if self.cli_probe.is_available() {
                    self.cli_probe.probe()
                } else {
                    Err(ProviderError::cli_not_found("claude").into())
                }
            }

            ProbeMode::Api => {
                debug!("Claude: 强制使用 API 模式");
                if self.api_probe.is_available() {
                    self.api_probe.probe()
                } else {
                    Err(ProviderError::auth_required(Some(
                        "未找到 OAuth 凭证，请运行 `claude` 登录",
                    ))
                    .into())
                }
            }

            ProbeMode::Auto => {
                let mut api_err = None;

                // 优先尝试 API
                if self.api_probe.is_available() {
                    debug!("Claude: Auto 模式，尝试 API...");
                    match self.api_probe.probe() {
                        Ok(quotas) => {
                            debug!("Claude: API 成功，返回 {} 个配额", quotas.len());
                            return Ok(quotas);
                        }
                        Err(e) => {
                            debug!("Claude: API 失败: {}，回退到 CLI", e);
                            api_err = Some(e);
                        }
                    }
                }

                // 回退到 CLI
                if self.cli_probe.is_available() {
                    debug!("Claude: Auto 模式，使用 CLI...");
                    return self.cli_probe.probe();
                }

                // 两种方式都不可用，优先返回 API 的原始错误
                if let Some(e) = api_err {
                    Err(e)
                } else {
                    Err(ProviderError::unavailable("Claude API 和 CLI 都不可用").into())
                }
            }
        }
    }
}
