use crate::models::{ErrorKind, ProviderFailure};
use crate::providers::ProviderError;

/// Provider 错误映射层。
///
/// Provider 只负责返回结构化错误；这里负责把 provider 层错误映射到
/// application/models 可持有的稳定 failure 语义。
pub struct ProviderErrorPresenter;

impl ProviderErrorPresenter {
    pub fn to_failure(error: &ProviderError) -> ProviderFailure {
        error.to_failure()
    }

    pub fn to_error_kind(error: &ProviderError) -> ErrorKind {
        error.error_kind()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{FailureReason, ProviderFailure};

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

    #[test]
    fn test_to_failure() {
        let failure = ProviderErrorPresenter::to_failure(&ProviderError::NoData);
        assert_eq!(
            failure,
            ProviderFailure {
                reason: FailureReason::NoData,
                advice: None,
                raw_detail: None,
            }
        );
    }
}
