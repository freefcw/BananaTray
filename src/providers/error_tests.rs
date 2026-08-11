use super::*;
use crate::models::FailureAdvice;

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

// ── classify + HttpError ────────────────────────────────

#[test]
fn test_classify_http_401_as_auth_required() {
    use crate::providers::common::http_client::HttpError;
    let err: anyhow::Error = HttpError::HttpStatus { code: 401 }.into();
    let classified = ProviderError::classify(&err);
    assert!(matches!(classified, ProviderError::AuthRequired { .. }));
}

#[test]
fn test_classify_http_403_as_auth_required() {
    use crate::providers::common::http_client::HttpError;
    let err: anyhow::Error = HttpError::HttpStatus { code: 403 }.into();
    let classified = ProviderError::classify(&err);
    assert!(matches!(classified, ProviderError::AuthRequired { .. }));
}

#[test]
fn test_classify_http_429_as_fetch_failed() {
    use crate::providers::common::http_client::HttpError;
    let err: anyhow::Error = HttpError::HttpStatus { code: 429 }.into();
    let classified = ProviderError::classify(&err);
    assert!(matches!(
        classified,
        ProviderError::FetchFailed {
            advice: Some(FailureAdvice::ApiHttpError { ref status }),
            raw_detail: Some(ref raw_detail),
        } if status == "429" && raw_detail.contains("429")
    ));
}

#[test]
fn test_classify_http_500_as_fetch_failed() {
    use crate::providers::common::http_client::HttpError;
    let err: anyhow::Error = HttpError::HttpStatus { code: 500 }.into();
    let classified = ProviderError::classify(&err);
    assert!(matches!(
        classified,
        ProviderError::FetchFailed {
            advice: Some(FailureAdvice::ApiHttpError { ref status }),
            raw_detail: Some(ref raw_detail),
        } if status == "500" && raw_detail.contains("500")
    ));
}

#[test]
fn test_classify_http_status_does_not_expose_response_body() {
    use crate::providers::common::http_client::HttpError;
    let secret = "token=secret-123 user@example.com 中文🍌";
    let err: anyhow::Error = HttpError::HttpStatus { code: 500 }.into();

    let classified = ProviderError::classify(&err);
    let failure = classified.to_failure();

    assert!(!classified.to_string().contains(secret));
    assert!(!failure
        .raw_detail
        .as_deref()
        .is_some_and(|detail| detail.contains(secret)));
}

#[test]
fn test_classify_http_timeout() {
    use crate::providers::common::http_client::HttpError;
    let err: anyhow::Error = HttpError::Timeout.into();
    let classified = ProviderError::classify(&err);
    assert!(matches!(classified, ProviderError::Timeout));
}

#[test]
fn test_classify_http_transport_as_network_failed() {
    use crate::providers::common::http_client::HttpError;
    let err: anyhow::Error = HttpError::Transport("DNS resolution failed".into()).into();
    let classified = ProviderError::classify(&err);
    assert!(matches!(classified, ProviderError::NetworkFailed { .. }));
}
