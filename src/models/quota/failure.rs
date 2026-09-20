use serde::{Deserialize, Serialize};

/// Provider 最近一次失败的稳定语义载荷。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderFailure {
    pub reason: FailureReason,
    #[serde(default)]
    pub advice: Option<FailureAdvice>,
    /// 已脱敏、仅供 Debug 诊断展示的技术细节。
    ///
    /// 禁止包含响应正文、凭据、配置值或其他敏感用户数据。
    #[serde(default)]
    pub raw_detail: Option<String>,
}

/// 失败原因主类型。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailureReason {
    CliNotFound { cli_name: String },
    AuthRequired,
    SessionExpired,
    FolderTrustRequired,
    UpdateRequired { version: Option<String> },
    ConfigMissing { key: String },
    Unavailable,
    ParseFailed,
    Timeout,
    NoData,
    NetworkFailed,
    FetchFailed,
}

/// Provider 建议动作/补充说明的稳定语义。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailureAdvice {
    LoginCli { cli: String },
    ReloginCli { cli: String },
    RefreshCli { cli: String },
    LoginApp { app: String },
    OpenAppToRefresh { app: String },
    CliExitFailed { code: i32 },
    ApiHttpError { status: String },
    // 上游 API 返回业务错误；仅保留稳定错误码，不携带响应原文。
    ApiError { code: i32 },
    NoOauthCreds { cli: String },
    BothUnavailable { name: String },
    TrustFolder { cli: String },
    CannotParseQuota,
    TokenStillInvalid,
    // 权限/订阅类失败：语义固定，文案由 selector 出，provider 侧不再拼英文句子。
    CopilotTokenNoPermission,
    CopilotNotEnabled,
    OpenCodeGoRequired,
}
