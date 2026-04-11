use crate::models::ErrorKind;
use crate::providers::error_presenter::ProviderErrorPresenter;

// ============================================================================
// ProviderError 分类测试（build_outcome 使用的错误转换）
// ============================================================================

#[test]
fn test_classify_error_kind_config_missing() {
    let error = crate::providers::ProviderError::ConfigMissing {
        key: "github_token".to_string(),
    };
    assert_eq!(
        ProviderErrorPresenter::to_error_kind(&error),
        ErrorKind::ConfigMissing
    );
}

#[test]
fn test_classify_error_kind_auth_required() {
    let error = crate::providers::ProviderError::AuthRequired { hint: None };
    assert_eq!(
        ProviderErrorPresenter::to_error_kind(&error),
        ErrorKind::AuthRequired
    );
}

#[test]
fn test_classify_error_kind_session_expired() {
    let error = crate::providers::ProviderError::SessionExpired {
        hint: Some("re-login".to_string()),
    };
    assert_eq!(
        ProviderErrorPresenter::to_error_kind(&error),
        ErrorKind::AuthRequired
    );
}

#[test]
fn test_classify_error_kind_network_error() {
    let error = crate::providers::ProviderError::Timeout;
    assert_eq!(
        ProviderErrorPresenter::to_error_kind(&error),
        ErrorKind::NetworkError
    );

    let error = crate::providers::ProviderError::NetworkFailed {
        reason: "timeout".to_string(),
    };
    assert_eq!(
        ProviderErrorPresenter::to_error_kind(&error),
        ErrorKind::NetworkError
    );
}

#[test]
fn test_classify_error_kind_unknown() {
    let error = crate::providers::ProviderError::CliNotFound {
        cli_name: "claude".to_string(),
    };
    assert_eq!(
        ProviderErrorPresenter::to_error_kind(&error),
        ErrorKind::Unknown
    );

    let error = crate::providers::ProviderError::ParseFailed {
        reason: "invalid json".to_string(),
    };
    assert_eq!(
        ProviderErrorPresenter::to_error_kind(&error),
        ErrorKind::Unknown
    );
}
