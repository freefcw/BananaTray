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
use rust_i18n::t;

use super::{AiProvider, ProviderError};
use crate::models::{ProviderDescriptor, ProviderKind, ProviderMetadata, RefreshData};
use anyhow::Result;

/// Claude Provider
///
/// 支持三种获取模式：
/// - Auto: 优先 API，失败回退 CLI（默认）
/// - Cli: 强制使用 CLI
/// - Api: 强制使用 API
pub struct ClaudeProvider {
    cli_probe: Box<dyn UsageProbe>,
    api_probe: Box<dyn UsageProbe>,
    probe_mode: ProbeMode,
}

impl Default for ClaudeProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ClaudeProvider {
    pub fn new() -> Self {
        Self::with_probes(
            Box::new(ClaudeCliProbe),
            Box::new(ClaudeApiProbe),
            ProbeMode::Auto,
        )
    }

    fn with_probes(
        cli_probe: Box<dyn UsageProbe>,
        api_probe: Box<dyn UsageProbe>,
        probe_mode: ProbeMode,
    ) -> Self {
        Self {
            cli_probe,
            api_probe,
            probe_mode,
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

    fn both_unavailable_error() -> anyhow::Error {
        ProviderError::unavailable(&t!("hint.both_unavailable", name = "Claude")).into()
    }

    fn ensure_cli_available(&self) -> Result<()> {
        if self.cli_probe.is_available() {
            Ok(())
        } else {
            Err(ProviderError::cli_not_found("claude").into())
        }
    }

    fn ensure_api_available(&self) -> Result<()> {
        if self.api_probe.is_available() {
            Ok(())
        } else {
            Err(
                ProviderError::auth_required(Some(&t!("hint.no_oauth_creds", cli = "claude")))
                    .into(),
            )
        }
    }

    fn refresh_from_probe(probe: &dyn UsageProbe) -> Result<RefreshData> {
        let quotas = probe.probe()?;
        Ok(RefreshData::quotas_only(quotas))
    }

    fn refresh_via_cli(&self) -> Result<RefreshData> {
        debug!("Claude: using CLI source");
        self.ensure_cli_available()?;
        Self::refresh_from_probe(self.cli_probe.as_ref())
    }

    fn refresh_via_api(&self) -> Result<RefreshData> {
        debug!("Claude: using API source");
        self.ensure_api_available()?;
        Self::refresh_from_probe(self.api_probe.as_ref())
    }

    fn refresh_auto(&self) -> Result<RefreshData> {
        if self.api_probe.is_available() {
            debug!("Claude: Auto mode, trying API source first");
            match Self::refresh_from_probe(self.api_probe.as_ref()) {
                Ok(data) => return Ok(data),
                Err(api_err) => {
                    debug!(
                        "Claude: API source failed: {}, trying CLI fallback",
                        api_err
                    );
                    if self.cli_probe.is_available() {
                        return Self::refresh_from_probe(self.cli_probe.as_ref());
                    }
                    return Err(api_err);
                }
            }
        }

        if self.cli_probe.is_available() {
            debug!("Claude: Auto mode, API unavailable, using CLI source");
            return Self::refresh_from_probe(self.cli_probe.as_ref());
        }

        Err(Self::both_unavailable_error())
    }
}

#[async_trait]
impl AiProvider for ClaudeProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: "claude",
            metadata: ProviderMetadata {
                kind: ProviderKind::Claude,
                display_name: "Claude".into(),
                brand_name: "Anthropic".into(),
                icon_asset: "src/icons/provider-claude.svg".into(),
                dashboard_url: "https://console.anthropic.com/settings/billing".into(),
                account_hint: "Anthropic workspace".into(),
                source_label: "claude".into(),
            },
        }
    }

    async fn check_availability(&self) -> Result<()> {
        if self.cli_probe.is_available() || self.api_probe.is_available() {
            Ok(())
        } else {
            Err(Self::both_unavailable_error())
        }
    }

    async fn refresh(&self) -> Result<RefreshData> {
        match self.probe_mode {
            ProbeMode::Cli => {
                debug!("Claude: forcing CLI mode");
                self.refresh_via_cli()
            }
            ProbeMode::Api => {
                debug!("Claude: forcing API mode");
                self.refresh_via_api()
            }
            ProbeMode::Auto => self.refresh_auto(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::QuotaInfo;

    struct StubProbe {
        available: bool,
        outcome: StubOutcome,
    }

    enum StubOutcome {
        Success { label: &'static str },
        Error(ProviderError),
    }

    impl UsageProbe for StubProbe {
        fn probe(&self) -> Result<Vec<QuotaInfo>> {
            match &self.outcome {
                StubOutcome::Success { label } => Ok(vec![QuotaInfo::new(*label, 10.0, 100.0)]),
                StubOutcome::Error(err) => Err(err.clone().into()),
            }
        }

        fn is_available(&self) -> bool {
            self.available
        }
    }

    fn provider(
        cli_probe: StubProbe,
        api_probe: StubProbe,
        probe_mode: ProbeMode,
    ) -> ClaudeProvider {
        ClaudeProvider::with_probes(Box::new(cli_probe), Box::new(api_probe), probe_mode)
    }

    #[test]
    fn test_auto_prefers_api_source() {
        let provider = provider(
            StubProbe {
                available: true,
                outcome: StubOutcome::Success { label: "cli" },
            },
            StubProbe {
                available: true,
                outcome: StubOutcome::Success { label: "api" },
            },
            ProbeMode::Auto,
        );

        let data = provider.refresh_auto().unwrap();
        assert_eq!(data.quotas.len(), 1);
        assert_eq!(data.quotas[0].label, "api");
    }

    #[test]
    fn test_auto_falls_back_to_cli_when_api_fails() {
        let provider = provider(
            StubProbe {
                available: true,
                outcome: StubOutcome::Success { label: "cli" },
            },
            StubProbe {
                available: true,
                outcome: StubOutcome::Error(ProviderError::fetch_failed("api down")),
            },
            ProbeMode::Auto,
        );

        let data = provider.refresh_auto().unwrap();
        assert_eq!(data.quotas[0].label, "cli");
    }

    #[test]
    fn test_auto_returns_api_error_when_cli_unavailable() {
        let provider = provider(
            StubProbe {
                available: false,
                outcome: StubOutcome::Success { label: "cli" },
            },
            StubProbe {
                available: true,
                outcome: StubOutcome::Error(ProviderError::session_expired(Some("expired"))),
            },
            ProbeMode::Auto,
        );

        let err = provider.refresh_auto().unwrap_err();
        let classified = ProviderError::classify(&err);
        assert!(matches!(classified, ProviderError::SessionExpired { .. }));
    }

    #[test]
    fn test_check_availability_accepts_any_source() {
        let provider = provider(
            StubProbe {
                available: true,
                outcome: StubOutcome::Success { label: "cli" },
            },
            StubProbe {
                available: false,
                outcome: StubOutcome::Success { label: "api" },
            },
            ProbeMode::Auto,
        );

        assert!(smol::block_on(provider.check_availability()).is_ok());
    }

    #[test]
    fn test_check_availability_rejects_when_no_source_available() {
        let provider = provider(
            StubProbe {
                available: false,
                outcome: StubOutcome::Success { label: "cli" },
            },
            StubProbe {
                available: false,
                outcome: StubOutcome::Success { label: "api" },
            },
            ProbeMode::Auto,
        );

        let err = smol::block_on(provider.check_availability()).unwrap_err();
        let classified = ProviderError::classify(&err);
        assert!(matches!(classified, ProviderError::Unavailable { .. }));
    }

    #[test]
    fn test_api_mode_requires_api_source() {
        let provider = provider(
            StubProbe {
                available: true,
                outcome: StubOutcome::Success { label: "cli" },
            },
            StubProbe {
                available: false,
                outcome: StubOutcome::Success { label: "api" },
            },
            ProbeMode::Api,
        );

        let err = provider.refresh_via_api().unwrap_err();
        let classified = ProviderError::classify(&err);
        assert!(matches!(classified, ProviderError::AuthRequired { .. }));
    }
}
