use serde::{Deserialize, Serialize};

/// Provider 最近一次失败的稳定语义载荷。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderFailure {
    pub reason: FailureReason,
    #[serde(default)]
    pub advice: Option<FailureAdvice>,
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
    CliExitFailed { code: i32 },
    ApiHttpError { status: String },
    ApiError { message: String },
    NoOauthCreds { cli: String },
    BothUnavailable { name: String },
    TrustFolder { cli: String },
    CannotParseQuota,
    TokenStillInvalid,
}
