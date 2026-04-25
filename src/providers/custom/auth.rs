use anyhow::Result;
use log::{debug, info, warn};

use crate::providers::common::http_client;
use crate::providers::ProviderError;

use super::extractor;
use super::json_file::read_json_file;
use super::log_utils::truncate_for_log;
use super::schema::{AuthDef, HeaderDef};
use super::url::{expand_env_vars, resolve_url};

/// 将 AuthDef + HeaderDef 列表转换为 "Name: Value" 格式的 header 字符串。
///
/// **注意**：对于 `Login` 认证类型，此函数会发起 HTTP 请求获取 token（I/O 副作用）。
pub(super) fn resolve_auth_headers(
    base_url: &Option<String>,
    auth: &Option<AuthDef>,
    headers: &[HeaderDef],
) -> Result<Vec<String>> {
    let mut result = Vec::new();

    if let Some(auth) = auth {
        match auth {
            AuthDef::Bearer { token } => {
                result.push(format!("Authorization: Bearer {}", token));
            }
            AuthDef::BearerEnv { env_var } => {
                let token = read_env(env_var)?;
                result.push(format!("Authorization: Bearer {}", token));
            }
            AuthDef::HeaderEnv { header, env_var } => {
                let value = read_env(env_var)?;
                result.push(format!("{}: {}", header, value));
            }
            AuthDef::FileToken { path, token_path } => {
                let token = read_file_token(path, token_path)?;
                result.push(format!("Authorization: Bearer {}", token));
            }
            AuthDef::Login {
                login_url,
                username,
                password,
                token_path,
            } => {
                let username = expand_env_vars(username);
                let password = expand_env_vars(password);
                let token = login_for_token(base_url, login_url, &username, &password, token_path)?;
                result.push(format!("Authorization: Bearer {}", token));
            }
            AuthDef::Cookie { value } => {
                result.push(format!("Cookie: {}", value));
            }
            AuthDef::SessionToken { token, cookie_name } => {
                result.push(format!("Cookie: {}={}", cookie_name, token));
            }
        }
    }

    for h in headers {
        let value = expand_env_vars(&h.value);
        result.push(format!("{}: {}", h.name, value));
    }

    Ok(result)
}

/// 通过登录接口获取 access token。
///
/// POST login_url + {"username":"..","password":".."} → 解析 JSON → 提取 token_path 对应的值。
fn login_for_token(
    base_url: &Option<String>,
    login_url: &str,
    username: &str,
    password: &str,
    token_path: &str,
) -> Result<String> {
    let body = serde_json::json!({
        "username": username,
        "password": password
    })
    .to_string();

    let login_url = resolve_url(base_url, login_url);
    info!(target: "providers::custom", "login: POST {} (user={})", login_url, username);

    let response = http_client::post_json(&login_url, &[], &body);
    match &response {
        Ok(body) => debug!(
            target: "providers::custom",
            "login response ({} bytes): {}",
            body.len(),
            truncate_for_log(body, 300)
        ),
        Err(e) => warn!(target: "providers::custom", "login request failed: {}", e),
    }
    let response = response?;

    let result = parse_login_response(&response, token_path);
    if let Err(ref e) = result {
        warn!(
            target: "providers::custom",
            "login token extraction failed: {} (token_path='{}', response: {})",
            e, token_path, truncate_for_log(&response, 200)
        );
    }
    result
}

/// 从登录响应 JSON 中提取 token（纯逻辑，无 I/O，可单元测试）。
fn parse_login_response(response: &str, token_path: &str) -> Result<String> {
    let json: serde_json::Value = serde_json::from_str(response)
        .map_err(|_| ProviderError::parse_failed("login response is not valid JSON"))?;

    if let Some(false) = json.get("success").and_then(|v| v.as_bool()) {
        let msg = json
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("login failed");
        anyhow::bail!("login failed: {}", msg);
    }

    extractor::json_string(&json, token_path).ok_or_else(|| {
        ProviderError::parse_failed(&format!(
            "token not found at path '{}' in login response",
            token_path
        ))
        .into()
    })
}

fn read_env(var: &str) -> Result<String> {
    std::env::var(var)
        .ok()
        .filter(|v| !v.is_empty())
        .ok_or_else(|| ProviderError::config_missing(var).into())
}

/// 从本地 JSON 文件中读取 token（纯 I/O + JSON 点分路径提取）。
fn read_file_token(path: &str, token_path: &str) -> Result<String> {
    let json = read_json_file(path)?;
    extractor::json_string(&json, token_path).ok_or_else(|| {
        ProviderError::config_missing(&format!("token not found at '{}' in {}", token_path, path))
            .into()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_auth_headers_bearer_env() {
        std::env::set_var("TEST_TOKEN_FETCHER", "tok123");
        let auth = Some(AuthDef::BearerEnv {
            env_var: "TEST_TOKEN_FETCHER".to_string(),
        });
        let headers = resolve_auth_headers(&None, &auth, &[]).unwrap();
        assert_eq!(headers, vec!["Authorization: Bearer tok123"]);
        std::env::remove_var("TEST_TOKEN_FETCHER");
    }

    #[test]
    fn test_resolve_auth_headers_missing_env() {
        std::env::remove_var("MISSING_TOKEN_12345");
        let auth = Some(AuthDef::BearerEnv {
            env_var: "MISSING_TOKEN_12345".to_string(),
        });
        assert!(resolve_auth_headers(&None, &auth, &[]).is_err());
    }

    #[test]
    fn test_resolve_auth_headers_custom_with_env_expansion() {
        std::env::set_var("TEST_CUSTOM_HEADER_VAL", "secret");
        let headers = vec![HeaderDef {
            name: "X-Custom".to_string(),
            value: "Bearer ${TEST_CUSTOM_HEADER_VAL}".to_string(),
        }];
        let result = resolve_auth_headers(&None, &None, &headers).unwrap();
        assert_eq!(result, vec!["X-Custom: Bearer secret"]);
        std::env::remove_var("TEST_CUSTOM_HEADER_VAL");
    }

    #[test]
    fn test_resolve_auth_headers_bearer_direct() {
        let auth = Some(AuthDef::Bearer {
            token: "sk-test-token-123".to_string(),
        });
        let headers = resolve_auth_headers(&None, &auth, &[]).unwrap();
        assert_eq!(headers, vec!["Authorization: Bearer sk-test-token-123"]);
    }

    #[test]
    fn test_resolve_auth_headers_cookie() {
        let auth = Some(AuthDef::Cookie {
            value: "session=eyJhbGci...;cf_clearance=abc".to_string(),
        });
        let headers = resolve_auth_headers(&None, &auth, &[]).unwrap();
        assert_eq!(
            headers,
            vec!["Cookie: session=eyJhbGci...;cf_clearance=abc"]
        );
    }

    #[test]
    fn test_resolve_auth_headers_session_token() {
        let auth = Some(AuthDef::SessionToken {
            token: "eyJhbGciOiJIUzI1NiJ9".to_string(),
            cookie_name: "session".to_string(),
        });
        let headers = resolve_auth_headers(&None, &auth, &[]).unwrap();
        assert_eq!(headers, vec!["Cookie: session=eyJhbGciOiJIUzI1NiJ9"]);
    }

    #[test]
    fn test_resolve_auth_headers_session_token_custom_name() {
        let auth = Some(AuthDef::SessionToken {
            token: "tok-abc-123".to_string(),
            cookie_name: "access_token".to_string(),
        });
        let headers = resolve_auth_headers(&None, &auth, &[]).unwrap();
        assert_eq!(headers, vec!["Cookie: access_token=tok-abc-123"]);
    }

    #[test]
    fn test_parse_login_response_success() {
        let response = r#"{"success": true, "data": "tok-abc-123"}"#;
        let token = parse_login_response(response, "data").unwrap();
        assert_eq!(token, "tok-abc-123");
    }

    #[test]
    fn test_parse_login_response_nested_token_path() {
        let response = r#"{"success": true, "data": {"access_token": "tok-xyz"}}"#;
        let token = parse_login_response(response, "data.access_token").unwrap();
        assert_eq!(token, "tok-xyz");
    }

    #[test]
    fn test_parse_login_response_failure_with_message() {
        let response = r#"{"success": false, "message": "invalid password"}"#;
        let err = parse_login_response(response, "data").unwrap_err();
        assert!(err.to_string().contains("invalid password"));
    }

    #[test]
    fn test_parse_login_response_failure_without_message() {
        let response = r#"{"success": false}"#;
        let err = parse_login_response(response, "data").unwrap_err();
        assert!(err.to_string().contains("login failed"));
    }

    #[test]
    fn test_parse_login_response_no_success_field_still_extracts_token() {
        let response = r#"{"data": "tok-no-success-field"}"#;
        let token = parse_login_response(response, "data").unwrap();
        assert_eq!(token, "tok-no-success-field");
    }

    #[test]
    fn test_parse_login_response_token_not_found() {
        let response = r#"{"success": true, "other": "value"}"#;
        let err = parse_login_response(response, "data").unwrap_err();
        assert!(err.to_string().contains("token not found"));
    }

    #[test]
    fn test_parse_login_response_invalid_json() {
        let err = parse_login_response("not json", "data").unwrap_err();
        assert!(err.to_string().contains("not valid JSON"));
    }

    #[test]
    fn test_resolve_auth_headers_login_env_expansion() {
        std::env::set_var("TEST_LOGIN_USER", "admin");
        std::env::set_var("TEST_LOGIN_PASS", "secret123");

        let username = expand_env_vars("${TEST_LOGIN_USER}");
        let password = expand_env_vars("${TEST_LOGIN_PASS}");
        assert_eq!(username, "admin");
        assert_eq!(password, "secret123");

        std::env::remove_var("TEST_LOGIN_USER");
        std::env::remove_var("TEST_LOGIN_PASS");
    }

    #[test]
    fn test_read_file_token_success() {
        let dir = tempfile::tempdir().unwrap();
        let auth_path = dir.path().join("auth.json");
        std::fs::write(
            &auth_path,
            r#"{"tokens":{"access_token":"sk-test-12345","refresh_token":"rt-abc"}}"#,
        )
        .unwrap();
        let token = read_file_token(auth_path.to_str().unwrap(), "tokens.access_token").unwrap();
        assert_eq!(token, "sk-test-12345");
    }

    #[test]
    fn test_read_file_token_path_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let auth_path = dir.path().join("auth.json");
        std::fs::write(&auth_path, r#"{"tokens":{"refresh_token":"rt-abc"}}"#).unwrap();
        let err = read_file_token(auth_path.to_str().unwrap(), "tokens.access_token").unwrap_err();
        assert!(err.to_string().contains("token not found"));
    }

    #[test]
    fn test_read_file_token_file_missing() {
        let err =
            read_file_token("/nonexistent/path/auth.json", "tokens.access_token").unwrap_err();
        assert!(err.to_string().contains("file not found"));
    }

    #[test]
    fn test_read_file_token_invalid_json() {
        let dir = tempfile::tempdir().unwrap();
        let auth_path = dir.path().join("auth.json");
        std::fs::write(&auth_path, "not json content").unwrap();
        let err = read_file_token(auth_path.to_str().unwrap(), "tokens.access_token").unwrap_err();
        assert!(err.to_string().contains("invalid JSON"));
    }
}
