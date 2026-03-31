pub mod manager;

use crate::models::{ProviderKind, ProviderMetadata, QuotaInfo, RefreshData};
use anyhow::Result;
use async_trait::async_trait;
use rust_i18n::t;
use std::sync::Arc;

pub use manager::ProviderManager;

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
/// - **面向用户的提示**（CliNotFound, AuthRequired, SessionExpired, FolderTrustRequired,
///   UpdateRequired, ConfigMissing）→ 在 `format_for_display()` 中国际化
/// - **技术性错误**（Unavailable, ParseFailed, Timeout, NoData, NetworkFailed,
///   FetchFailed）→ 保留英文 reason，便于调试
#[derive(Debug, Clone)]
#[allow(dead_code)] // 某些变体预留给未来使用
pub enum ProviderError {
    // ── 面向用户的提示（国际化）──────────────────────────
    /// CLI 未安装或找不到
    CliNotFound { cli_name: String },
    /// 需要登录认证
    AuthRequired { hint: Option<String> },
    /// OAuth 会话已过期
    SessionExpired { hint: Option<String> },
    /// 需要信任文件夹（Claude CLI 特有）
    FolderTrustRequired,
    /// CLI 需要更新
    UpdateRequired { version: Option<String> },
    /// 配置缺失（环境变量、配置文件、Token 等）
    ConfigMissing { key: String },

    // ── 技术性错误（不国际化，保留英文）────────────────────
    /// Provider 在当前环境不可用（文件不存在、服务未运行等）
    Unavailable { reason: String },
    /// 解析响应失败
    ParseFailed { reason: String },
    /// 网络请求超时
    Timeout,
    /// 无配额数据
    NoData,
    /// 网络请求失败
    NetworkFailed { reason: String },
    /// 其他获取失败
    FetchFailed { reason: String },
}

impl std::fmt::Display for ProviderError {
    /// 英文技术描述，面向日志和开发者调试
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CliNotFound { cli_name } => {
                write!(f, "CLI not found: {}", cli_name)
            }
            Self::Unavailable { reason } => {
                write!(f, "unavailable: {}", reason)
            }
            Self::AuthRequired { hint } => {
                let msg = hint.as_deref().unwrap_or("please run login command");
                write!(f, "auth required: {}", msg)
            }
            Self::SessionExpired { hint } => {
                let msg = hint.as_deref().unwrap_or("please re-login");
                write!(f, "session expired: {}", msg)
            }
            Self::FolderTrustRequired => {
                write!(f, "folder trust required")
            }
            Self::UpdateRequired { version } => match version {
                Some(v) => write!(f, "update required: version {}", v),
                None => write!(f, "update required: latest version"),
            },
            Self::ParseFailed { reason } => {
                write!(f, "parse failed: {}", reason)
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
            Self::FetchFailed { reason } => {
                write!(f, "fetch failed: {}", reason)
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
            reason: err.to_string(),
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
            reason: reason.to_string(),
        }
    }

    /// 需要认证
    pub fn auth_required(hint: Option<&str>) -> Self {
        Self::AuthRequired {
            hint: hint.map(|s| s.to_string()),
        }
    }

    /// 会话过期
    pub fn session_expired(hint: Option<&str>) -> Self {
        Self::SessionExpired {
            hint: hint.map(|s| s.to_string()),
        }
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
            reason: reason.to_string(),
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
            reason: reason.to_string(),
        }
    }

    /// 返回用户友好的错误消息（UI 展示用）
    ///
    /// 设计原则：
    /// - 面向用户的提示 → 国际化
    /// - 技术性错误 → 直接返回英文 reason（便于调试和搜索）
    pub fn format_for_display(&self) -> String {
        match self {
            // ── 面向用户的提示（国际化）──────────────────────────
            Self::CliNotFound { cli_name } => t!("error.cli_not_found", cli = cli_name).to_string(),
            Self::AuthRequired { hint } => match hint {
                Some(h) => h.clone(), // hint 已经是国际化的文本
                None => t!("error.auth_required_default").to_string(),
            },
            Self::SessionExpired { hint } => match hint {
                Some(h) => h.clone(), // hint 已经是国际化的文本
                None => t!("error.session_expired_default").to_string(),
            },
            Self::FolderTrustRequired => t!("error.folder_trust").to_string(),
            Self::UpdateRequired { version } => match version {
                Some(v) => t!("error.update_required_ver", version = v).to_string(),
                None => t!("error.update_required").to_string(),
            },
            Self::ConfigMissing { key } => t!("error.config_missing", key = key).to_string(),

            // ── 技术性错误（保留英文，便于调试）────────────────────
            Self::Unavailable { reason } => reason.clone(),
            Self::ParseFailed { reason } => reason.clone(),
            Self::Timeout => "Request timeout".to_string(),
            Self::NoData => "No quota data available".to_string(),
            Self::NetworkFailed { reason } => format!("Network error: {}", reason),
            Self::FetchFailed { reason } => reason.clone(),
        }
    }
}

/// AI Provider 的核心接口
#[async_trait]
pub trait AiProvider: Send + Sync {
    /// 获取 Provider 的元数据
    fn metadata(&self) -> ProviderMetadata;

    /// 该 Provider 的内部唯一标识（通常与 kind().id_key() 不同，用于更细粒度的区分实现）
    fn id(&self) -> &'static str;

    /// 关联的枚举类型
    fn kind(&self) -> ProviderKind {
        self.metadata().kind
    }

    /// 是否可以在当前设备或环境中可用（如 CLI 是否已安装等）
    async fn is_available(&self) -> bool {
        true
    }

    /// 核心方法：拉取最新的配额/用量情况
    async fn refresh(&self) -> Result<RefreshData> {
        // 默认实现：调用 refresh_quotas 并包装为 RefreshData
        let quotas = self.refresh_quotas().await?;
        Ok(RefreshData::quotas_only(quotas))
    }

    /// 辅助方法：只返回配额列表（向后兼容）
    /// 如果 Provider 没有账户信息需求，可以实现这个方法
    async fn refresh_quotas(&self) -> Result<Vec<QuotaInfo>> {
        Err(anyhow::anyhow!(
            "Provider must implement either refresh or refresh_quotas"
        ))
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
        let err = ProviderError::auth_required(Some("run `claude` to login"));
        assert_eq!(err.to_string(), "auth required: run `claude` to login");
    }

    #[test]
    fn test_display_auth_required_without_hint() {
        let err = ProviderError::auth_required(None);
        assert_eq!(err.to_string(), "auth required: please run login command");
    }

    #[test]
    fn test_display_session_expired() {
        let err = ProviderError::session_expired(Some("run `codex` to re-login"));
        assert_eq!(err.to_string(), "session expired: run `codex` to re-login");
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
        let original = ProviderError::session_expired(Some("test"));
        let anyhow_err: anyhow::Error = original.into();
        let classified = ProviderError::classify(&anyhow_err);
        assert!(matches!(classified, ProviderError::SessionExpired { .. }));
    }
}
