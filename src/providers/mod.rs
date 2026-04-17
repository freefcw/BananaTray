pub mod codeium_family;
pub mod common;
pub mod custom;
pub mod error_presenter;
pub mod manager;

use crate::models::{
    AppSettings, ErrorKind, FailureAdvice, FailureReason, ProviderDescriptor, ProviderFailure,
    RefreshData, SettingsCapability, TokenEditMode, TokenInputState,
};
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;

pub use manager::{ProviderManager, ProviderManagerHandle};

pub(crate) fn default_token_input_state(
    settings: &AppSettings,
    credential_key: &'static str,
) -> TokenInputState {
    let value = settings.provider.credentials.get_credential(credential_key);
    let has_token = value.is_some();
    TokenInputState {
        has_token,
        masked: value.map(mask_token),
        source_i18n_key: None,
        edit_mode: if has_token {
            TokenEditMode::EditStored
        } else {
            TokenEditMode::SetNew
        },
    }
}

fn mask_token(token: &str) -> String {
    let chars: Vec<char> = token.chars().collect();
    if chars.len() <= 8 {
        "•".repeat(chars.len())
    } else {
        let prefix: String = chars[..4].iter().collect();
        let suffix: String = chars[chars.len() - 4..].iter().collect();
        format!("{}•••{}", prefix, suffix)
    }
}

/// 消除零字段 Provider 的重复样板代码（struct + Default + new）
macro_rules! define_unit_provider {
    ($name:ident) => {
        pub struct $name;

        impl Default for $name {
            fn default() -> Self {
                Self
            }
        }

        impl $name {
            pub fn new() -> Self {
                Self
            }
        }
    };
}
pub(crate) use define_unit_provider;

/// Provider 刷新失败的结构化错误类型
///
/// 设计原则：
/// - Provider 层只返回稳定语义，不直接生成最终展示文案
/// - selector/UI 再基于 `ProviderFailure` 和当前 locale 生成字符串
/// - `raw_detail` 只承载技术细节或上游原文，不承载本地化外壳文案
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)] // 某些变体预留给未来使用
pub enum ProviderError {
    // ── 面向用户的提示（国际化）──────────────────────────
    /// CLI 未安装或找不到
    CliNotFound { cli_name: String },
    /// 需要登录认证
    AuthRequired { advice: Option<FailureAdvice> },
    /// OAuth 会话已过期
    SessionExpired { advice: Option<FailureAdvice> },
    /// 需要信任文件夹（Claude CLI 特有）
    FolderTrustRequired,
    /// CLI 需要更新
    UpdateRequired { version: Option<String> },
    /// 配置缺失（环境变量、配置文件、Token 等）
    ConfigMissing { key: String },

    // ── 技术性错误（不国际化，保留英文）────────────────────
    /// Provider 在当前环境不可用（文件不存在、服务未运行等）
    Unavailable {
        advice: Option<FailureAdvice>,
        raw_detail: Option<String>,
    },
    /// 解析响应失败
    ParseFailed {
        advice: Option<FailureAdvice>,
        raw_detail: Option<String>,
    },
    /// 网络请求超时
    Timeout,
    /// 无配额数据
    NoData,
    /// 网络请求失败
    NetworkFailed { reason: String },
    /// 其他获取失败
    FetchFailed {
        advice: Option<FailureAdvice>,
        raw_detail: Option<String>,
    },
}

impl std::fmt::Display for ProviderError {
    /// 英文技术描述，面向日志和开发者调试
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CliNotFound { cli_name } => {
                write!(f, "CLI not found: {}", cli_name)
            }
            Self::Unavailable { advice, raw_detail } => {
                write!(f, "unavailable")?;
                if let Some(advice) = advice {
                    write!(f, " ({})", advice.summary())?;
                }
                if let Some(detail) = raw_detail {
                    write!(f, ": {}", detail)?;
                }
                Ok(())
            }
            Self::AuthRequired { advice } => {
                write!(f, "auth required")?;
                if let Some(advice) = advice {
                    write!(f, ": {}", advice.summary())?;
                }
                Ok(())
            }
            Self::SessionExpired { advice } => {
                write!(f, "session expired")?;
                if let Some(advice) = advice {
                    write!(f, ": {}", advice.summary())?;
                }
                Ok(())
            }
            Self::FolderTrustRequired => {
                write!(f, "folder trust required")
            }
            Self::UpdateRequired { version } => match version {
                Some(v) => write!(f, "update required: version {}", v),
                None => write!(f, "update required: latest version"),
            },
            Self::ParseFailed { advice, raw_detail } => {
                write!(f, "parse failed")?;
                if let Some(advice) = advice {
                    write!(f, " ({})", advice.summary())?;
                }
                if let Some(detail) = raw_detail {
                    write!(f, ": {}", detail)?;
                }
                Ok(())
            }
            Self::Timeout => {
                write!(f, "request timeout")
            }
            Self::NoData => {
                write!(f, "no quota data")
            }
            Self::NetworkFailed { reason } => {
                write!(f, "network error: {}", reason)
            }
            Self::ConfigMissing { key } => {
                write!(f, "config missing: {}", key)
            }
            Self::FetchFailed { advice, raw_detail } => {
                write!(f, "fetch failed")?;
                if let Some(advice) = advice {
                    write!(f, " ({})", advice.summary())?;
                }
                if let Some(detail) = raw_detail {
                    write!(f, ": {}", detail)?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for ProviderError {}

impl ProviderError {
    /// 从 anyhow::Error 提取错误类型。
    ///
    /// 设计原则：
    /// - Provider 应直接返回 `ProviderError`（推荐）
    /// - 非 `ProviderError` 的错误统一归类为 `FetchFailed`
    ///
    /// 注意：此方法不再进行字符串匹配，因为：
    /// 1. 所有 Provider 都已直接返回 `ProviderError`
    /// 2. 字符串匹配不可靠且违反 OCP
    /// 3. Display 输出是英文技术描述，不适合直接展示给用户
    pub fn classify(err: &anyhow::Error) -> Self {
        // 只检查是否已经是 ProviderError
        if let Some(provider_error) = err.downcast_ref::<Self>() {
            return provider_error.clone();
        }

        // 非 ProviderError 错误统一归类为 FetchFailed
        Self::FetchFailed {
            advice: None,
            raw_detail: Some(err.to_string()),
        }
    }

    /// CLI 未找到
    pub fn cli_not_found(cli_name: &str) -> Self {
        Self::CliNotFound {
            cli_name: cli_name.to_string(),
        }
    }

    /// Provider 不可用
    pub fn unavailable(reason: &str) -> Self {
        Self::Unavailable {
            advice: None,
            raw_detail: Some(reason.to_string()),
        }
    }

    /// Provider 不可用（结构化建议）
    pub fn unavailable_with_advice(advice: FailureAdvice) -> Self {
        Self::Unavailable {
            advice: Some(advice),
            raw_detail: None,
        }
    }

    /// 需要认证
    pub fn auth_required(advice: Option<FailureAdvice>) -> Self {
        Self::AuthRequired { advice }
    }

    /// 会话过期
    pub fn session_expired(advice: Option<FailureAdvice>) -> Self {
        Self::SessionExpired { advice }
    }

    /// 需要更新
    pub fn update_required(version: Option<&str>) -> Self {
        Self::UpdateRequired {
            version: version.map(|s| s.to_string()),
        }
    }

    /// 解析失败
    pub fn parse_failed(reason: &str) -> Self {
        Self::ParseFailed {
            advice: None,
            raw_detail: Some(reason.to_string()),
        }
    }

    /// 解析失败（结构化建议）
    pub fn parse_failed_with_advice(advice: FailureAdvice) -> Self {
        Self::ParseFailed {
            advice: Some(advice),
            raw_detail: None,
        }
    }

    /// 配置缺失
    pub fn config_missing(key: &str) -> Self {
        Self::ConfigMissing {
            key: key.to_string(),
        }
    }

    /// 无数据
    pub fn no_data() -> Self {
        Self::NoData
    }

    /// 获取失败（通用）
    pub fn fetch_failed(reason: &str) -> Self {
        Self::FetchFailed {
            advice: None,
            raw_detail: Some(reason.to_string()),
        }
    }

    /// 获取失败（结构化建议）
    pub fn fetch_failed_with_advice(advice: FailureAdvice) -> Self {
        Self::FetchFailed {
            advice: Some(advice),
            raw_detail: None,
        }
    }

    /// 将 provider 层错误转换为状态/UI 可持有的稳定语义。
    pub fn to_failure(&self) -> ProviderFailure {
        match self {
            Self::CliNotFound { cli_name } => ProviderFailure {
                reason: FailureReason::CliNotFound {
                    cli_name: cli_name.clone(),
                },
                advice: None,
                raw_detail: None,
            },
            Self::AuthRequired { advice } => ProviderFailure {
                reason: FailureReason::AuthRequired,
                advice: advice.clone(),
                raw_detail: None,
            },
            Self::SessionExpired { advice } => ProviderFailure {
                reason: FailureReason::SessionExpired,
                advice: advice.clone(),
                raw_detail: None,
            },
            Self::FolderTrustRequired => ProviderFailure {
                reason: FailureReason::FolderTrustRequired,
                advice: None,
                raw_detail: None,
            },
            Self::UpdateRequired { version } => ProviderFailure {
                reason: FailureReason::UpdateRequired {
                    version: version.clone(),
                },
                advice: None,
                raw_detail: None,
            },
            Self::ConfigMissing { key } => ProviderFailure {
                reason: FailureReason::ConfigMissing { key: key.clone() },
                advice: None,
                raw_detail: None,
            },
            Self::Unavailable { advice, raw_detail } => ProviderFailure {
                reason: FailureReason::Unavailable,
                advice: advice.clone(),
                raw_detail: raw_detail.clone(),
            },
            Self::ParseFailed { advice, raw_detail } => ProviderFailure {
                reason: FailureReason::ParseFailed,
                advice: advice.clone(),
                raw_detail: raw_detail.clone(),
            },
            Self::Timeout => ProviderFailure {
                reason: FailureReason::Timeout,
                advice: None,
                raw_detail: None,
            },
            Self::NoData => ProviderFailure {
                reason: FailureReason::NoData,
                advice: None,
                raw_detail: None,
            },
            Self::NetworkFailed { reason } => ProviderFailure {
                reason: FailureReason::NetworkFailed,
                advice: None,
                raw_detail: Some(reason.clone()),
            },
            Self::FetchFailed { advice, raw_detail } => ProviderFailure {
                reason: FailureReason::FetchFailed,
                advice: advice.clone(),
                raw_detail: raw_detail.clone(),
            },
        }
    }

    pub fn error_kind(&self) -> ErrorKind {
        match self {
            Self::ConfigMissing { .. } => ErrorKind::ConfigMissing,
            Self::AuthRequired { .. } | Self::SessionExpired { .. } => ErrorKind::AuthRequired,
            Self::Timeout | Self::NetworkFailed { .. } => ErrorKind::NetworkError,
            _ => ErrorKind::Unknown,
        }
    }
}

impl FailureAdvice {
    fn summary(&self) -> String {
        match self {
            Self::LoginCli { cli } => format!("login cli {}", cli),
            Self::ReloginCli { cli } => format!("relogin cli {}", cli),
            Self::RefreshCli { cli } => format!("refresh cli {}", cli),
            Self::LoginApp { app } => format!("login app {}", app),
            Self::CliExitFailed { code } => format!("cli exit {}", code),
            Self::ApiHttpError { status } => format!("http {}", status),
            Self::ApiError { message } => format!("api error {}", message),
            Self::NoOauthCreds { cli } => format!("missing oauth creds {}", cli),
            Self::BothUnavailable { name } => format!("both unavailable {}", name),
            Self::TrustFolder { cli } => format!("trust folder {}", cli),
            Self::CannotParseQuota => "cannot parse quota".to_string(),
            Self::TokenStillInvalid => "token still invalid".to_string(),
        }
    }
}

/// AI Provider 的核心接口
#[async_trait]
pub trait AiProvider: Send + Sync {
    /// 获取 Provider 的描述符（ID + 元数据）
    fn descriptor(&self) -> ProviderDescriptor;

    /// 检查当前环境是否满足刷新条件。
    async fn check_availability(&self) -> Result<()> {
        Ok(())
    }

    /// 核心方法：拉取最新的配额/用量情况
    async fn refresh(&self) -> Result<RefreshData>;

    /// 声明该 Provider 的设置 UI 能力（默认无交互设置）
    ///
    /// 返回 `SettingsCapability::TokenInput` 即可让 Settings UI 自动显示
    /// Token 输入面板，无需在 selector 或 UI 层硬编码。
    fn settings_capability(&self) -> SettingsCapability {
        SettingsCapability::None
    }

    /// 解析 TokenInput 面板的运行时展示状态。
    ///
    /// 默认行为：若 provider 声明了 `SettingsCapability::TokenInput`，
    /// 则仅从 settings 中读取该 credential 的当前值。
    fn resolve_token_input_state(&self, settings: &AppSettings) -> Option<TokenInputState> {
        match self.settings_capability() {
            SettingsCapability::TokenInput(config) => {
                Some(default_token_input_state(settings, config.credential_key))
            }
            _ => None,
        }
    }
}

macro_rules! register_providers {
    ($($mod_name:ident => $struct_name:ident),* $(,)?) => {
        $(pub mod $mod_name;)*

        /// 注册所有可用的 Provider 实现
        pub fn register_all(manager: &mut ProviderManager) {
            $(
                manager.register(Arc::new($mod_name::$struct_name::new()));
            )*
        }
    };
}

register_providers!(
    amp => AmpProvider,
    antigravity => AntigravityProvider,
    claude => ClaudeProvider,
    codex => CodexProvider,
    copilot => CopilotProvider,
    cursor => CursorProvider,
    gemini => GeminiProvider,
    kilo => KiloProvider,
    kimi => KimiProvider,
    kiro => KiroProvider,
    minimax => MiniMaxProvider,
    opencode => OpenCodeProvider,
    vertex_ai => VertexAiProvider,
    windsurf => WindsurfProvider,
);

#[cfg(test)]
mod tests {
    use super::*;

    // ── Display（英文技术描述） ────────────────────────────

    #[test]
    fn test_display_cli_not_found() {
        let err = ProviderError::cli_not_found("claude");
        assert_eq!(err.to_string(), "CLI not found: claude");
    }

    #[test]
    fn test_display_auth_required_with_hint() {
        let err = ProviderError::auth_required(Some(FailureAdvice::LoginCli {
            cli: "claude".to_string(),
        }));
        assert_eq!(err.to_string(), "auth required: login cli claude");
    }

    #[test]
    fn test_display_auth_required_without_hint() {
        let err = ProviderError::auth_required(None);
        assert_eq!(err.to_string(), "auth required");
    }

    #[test]
    fn test_display_session_expired() {
        let err = ProviderError::session_expired(Some(FailureAdvice::ReloginCli {
            cli: "codex".to_string(),
        }));
        assert_eq!(err.to_string(), "session expired: relogin cli codex");
    }

    #[test]
    fn test_display_config_missing() {
        let err = ProviderError::config_missing("KIMI_AUTH_TOKEN");
        assert_eq!(err.to_string(), "config missing: KIMI_AUTH_TOKEN");
    }

    #[test]
    fn test_display_parse_failed() {
        let err = ProviderError::parse_failed("invalid JSON");
        assert_eq!(err.to_string(), "parse failed: invalid JSON");
    }

    #[test]
    fn test_display_update_required() {
        let err = ProviderError::update_required(Some("v2.0.0"));
        assert_eq!(err.to_string(), "update required: version v2.0.0");
    }

    #[test]
    fn test_display_update_required_no_version() {
        let err = ProviderError::update_required(None);
        assert_eq!(err.to_string(), "update required: latest version");
    }

    #[test]
    fn test_display_unavailable() {
        let err = ProviderError::unavailable("service not running");
        assert_eq!(err.to_string(), "unavailable: service not running");
    }

    #[test]
    fn test_display_no_data() {
        let err = ProviderError::no_data();
        assert_eq!(err.to_string(), "no quota data");
    }

    #[test]
    fn test_display_timeout() {
        let err = ProviderError::Timeout;
        assert_eq!(err.to_string(), "request timeout");
    }

    #[test]
    fn test_display_fetch_failed() {
        let err = ProviderError::fetch_failed("network error");
        assert_eq!(err.to_string(), "fetch failed: network error");
    }

    // ── classify ──────────────────────────────────────────

    #[test]
    fn test_classify_provider_error() {
        let original = ProviderError::cli_not_found("claude");
        let anyhow_err: anyhow::Error = original.clone().into();
        let classified = ProviderError::classify(&anyhow_err);
        assert!(matches!(classified, ProviderError::CliNotFound { .. }));
    }

    #[test]
    fn test_classify_generic_error() {
        let anyhow_err: anyhow::Error = anyhow::anyhow!("some random error");
        let classified = ProviderError::classify(&anyhow_err);
        assert!(matches!(classified, ProviderError::FetchFailed { .. }));
    }

    #[test]
    fn test_error_chain() {
        // 测试错误可以正确转换为 anyhow::Error 并恢复
        let original =
            ProviderError::session_expired(Some(FailureAdvice::LoginCli { cli: "test".into() }));
        let anyhow_err: anyhow::Error = original.into();
        let classified = ProviderError::classify(&anyhow_err);
        assert!(matches!(classified, ProviderError::SessionExpired { .. }));
    }
}
