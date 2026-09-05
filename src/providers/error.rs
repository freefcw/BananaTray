use crate::models::{ErrorKind, FailureAdvice, FailureReason, ProviderFailure};

/// Provider 刷新失败的结构化错误类型
///
/// 设计原则：
/// - Provider 层只返回稳定语义，不直接生成最终展示文案
/// - selector/UI 再基于 `ProviderFailure` 和当前 locale 生成字符串
/// - `raw_detail` 只承载技术细节或上游原文，不承载本地化外壳文案
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderError {
    // ── 面向用户的提示（国际化）──────────────────────────
    /// CLI 未安装或找不到
    CliNotFound { cli_name: String },
    /// 需要登录认证
    AuthRequired { advice: Option<FailureAdvice> },
    /// OAuth 会话已过期
    SessionExpired { advice: Option<FailureAdvice> },
    /// 需要信任文件夹（Claude CLI 特有）
    #[allow(dead_code)] // 预留给 Claude CLI trust-flow 场景
    FolderTrustRequired,
    /// CLI 需要更新
    #[allow(dead_code)] // 预留给 CLI 版本检测场景
    UpdateRequired { version: Option<String> },
    /// 配置缺失（环境变量、配置文件、Token 等）
    ConfigMissing { key: String },
    /// 配置存在但与 Provider 所需模式不匹配；仅保留稳定位置，不携带配置值。
    ConfigMismatch { location: String },

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

/// Provider facade 对外返回的结构化结果类型。
///
/// 内部 helper 仍可使用 `anyhow::Result` 承载技术上下文，但跨过 provider facade
/// 进入 refresh/runtime 边界时必须收敛为 `ProviderError`。
pub type ProviderResult<T> = std::result::Result<T, ProviderError>;

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
            Self::ConfigMismatch { location } => {
                write!(f, "config mismatch: {}", location)
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

impl From<anyhow::Error> for ProviderError {
    fn from(err: anyhow::Error) -> Self {
        Self::classify(&err)
    }
}

impl ProviderError {
    /// 从 anyhow::Error 提取错误类型。
    ///
    /// 分类优先级：
    /// 1. 已经是 `ProviderError` → 直接使用
    /// 2. 是 `HttpError` → 按状态码映射（401/403 → AuthRequired, 超时 → Timeout, 其余 → FetchFailed）
    /// 3. 其他 → `FetchFailed`
    pub fn classify(err: &anyhow::Error) -> Self {
        use crate::providers::common::http_client::HttpError;

        // 1. 已经是 ProviderError
        if let Some(provider_error) = err.downcast_ref::<Self>() {
            return provider_error.clone();
        }

        // 2. HTTP 结构化错误 → 按状态码分类
        if let Some(http_error) = err.downcast_ref::<HttpError>() {
            return match http_error {
                HttpError::Timeout => Self::Timeout,
                HttpError::Transport(reason) => Self::NetworkFailed {
                    reason: reason.clone(),
                },
                HttpError::HttpStatus { code } => match *code {
                    401 | 403 => Self::AuthRequired { advice: None },
                    _ => Self::FetchFailed {
                        advice: Some(FailureAdvice::ApiHttpError {
                            status: code.to_string(),
                        }),
                        // 上游正文不跨越 provider 边界，避免进入日志或用户界面。
                        raw_detail: Some(format!("HTTP {}", code)),
                    },
                },
            };
        }

        // 3. 其他错误统一归类为 FetchFailed
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

    /// 配置值不符合 Provider 要求。
    ///
    /// `location` 只描述文件和字段位置；不得放入 expected / actual 配置值，
    /// 避免错误日志与 UI 载荷泄露凭据或其他敏感配置。
    pub fn config_mismatch(location: &str) -> Self {
        Self::ConfigMismatch {
            location: location.to_string(),
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
            Self::ConfigMismatch { location } => ProviderFailure {
                // 现有 UI 使用 ConfigMissing 的定位载荷展示可操作配置位置；
                // mismatch 保持相同用户语义，但不会把配置值带出 provider 层。
                reason: FailureReason::ConfigMissing {
                    key: location.clone(),
                },
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
            Self::ConfigMissing { .. } | Self::ConfigMismatch { .. } => ErrorKind::ConfigMissing,
            Self::AuthRequired { .. } | Self::SessionExpired { .. } => ErrorKind::AuthRequired,
            Self::Timeout | Self::NetworkFailed { .. } => ErrorKind::NetworkError,
            _ => ErrorKind::Unknown,
        }
    }
}

impl FailureAdvice {
    pub(crate) fn summary(&self) -> String {
        match self {
            Self::LoginCli { cli } => format!("login cli {}", cli),
            Self::ReloginCli { cli } => format!("relogin cli {}", cli),
            Self::RefreshCli { cli } => format!("refresh cli {}", cli),
            Self::LoginApp { app } => format!("login app {}", app),
            Self::OpenAppToRefresh { app } => format!("open app {} to refresh", app),
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

#[cfg(test)]
#[path = "error_tests.rs"]
mod tests;
