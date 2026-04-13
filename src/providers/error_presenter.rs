use crate::models::ErrorKind;
use crate::providers::ProviderError;
use rust_i18n::t;

/// Provider 错误展示层。
///
/// Provider 只负责返回结构化错误，具体展示文本和 UI 分类由上层统一决定。
pub struct ProviderErrorPresenter;

impl ProviderErrorPresenter {
    pub fn to_message(error: &ProviderError) -> String {
        match error {
            ProviderError::CliNotFound { cli_name } => {
                t!("error.cli_not_found", cli = cli_name).to_string()
            }
            ProviderError::AuthRequired { hint } => hint
                .clone()
                .unwrap_or_else(|| t!("error.auth_required_default").to_string()),
            ProviderError::SessionExpired { hint } => hint
                .clone()
                .unwrap_or_else(|| t!("error.session_expired_default").to_string()),
            ProviderError::FolderTrustRequired => t!("error.folder_trust").to_string(),
            ProviderError::UpdateRequired { version } => match version {
                Some(v) => t!("error.update_required_ver", version = v).to_string(),
                None => t!("error.update_required").to_string(),
            },
            ProviderError::ConfigMissing { key } => {
                t!("error.config_missing", key = key).to_string()
            }
            ProviderError::Unavailable { reason } => reason.clone(),
            ProviderError::ParseFailed { reason } => reason.clone(),
            ProviderError::Timeout => t!("error.timeout").to_string(),
            ProviderError::NoData => t!("error.no_data").to_string(),
            ProviderError::NetworkFailed { reason } => {
                t!("error.network_failed", reason = reason).to_string()
            }
            ProviderError::FetchFailed { reason } => reason.clone(),
        }
    }

    pub fn to_error_kind(error: &ProviderError) -> ErrorKind {
        match error {
            ProviderError::ConfigMissing { .. } => ErrorKind::ConfigMissing,
            ProviderError::AuthRequired { .. } | ProviderError::SessionExpired { .. } => {
                ErrorKind::AuthRequired
            }
            ProviderError::Timeout | ProviderError::NetworkFailed { .. } => ErrorKind::NetworkError,
            _ => ErrorKind::Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_error_kind() {
        let error = ProviderError::ConfigMissing {
            key: "github_token".to_string(),
        };
        assert_eq!(
            ProviderErrorPresenter::to_error_kind(&error),
            ErrorKind::ConfigMissing
        );

        let error = ProviderError::Timeout;
        assert_eq!(
            ProviderErrorPresenter::to_error_kind(&error),
            ErrorKind::NetworkError
        );
    }
}
