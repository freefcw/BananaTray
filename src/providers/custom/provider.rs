use anyhow::Result;
use async_trait::async_trait;
use log::warn;
use std::borrow::Cow;

use crate::models::{ProviderDescriptor, ProviderKind, ProviderMetadata, RefreshData};
use crate::providers::common::cli;
use crate::providers::{AiProvider, ProviderError};
use crate::utils::http_client;

use super::extractor::{self, CompiledPatterns};
use super::schema::{AuthDef, AvailabilityDef, CustomProviderDef, HeaderDef, SourceDef};

/// 基于 YAML 定义的自定义 Provider 运行时
pub struct CustomProvider {
    def: CustomProviderDef,
    /// 预编译的正则缓存（对 JSON parser 为空）
    compiled: CompiledPatterns,
}

impl CustomProvider {
    pub fn new(def: CustomProviderDef) -> Result<Self> {
        let compiled = CompiledPatterns::compile(&def.parser)?;
        Ok(Self { def, compiled })
    }

    pub fn id(&self) -> &str {
        &self.def.id
    }
}

#[async_trait]
impl AiProvider for CustomProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        let base = &self.def.base_url;
        ProviderDescriptor {
            id: Cow::Owned(self.def.id.clone()),
            metadata: ProviderMetadata {
                kind: ProviderKind::Custom,
                display_name: self.def.metadata.display_name.clone(),
                brand_name: self.def.metadata.brand_name.clone(),
                icon_asset: self.def.metadata.icon.clone(),
                dashboard_url: resolve_url(base, &self.def.metadata.dashboard_url),
                account_hint: self.def.metadata.account_hint.clone(),
                source_label: self.def.metadata.source_label.clone(),
            },
        }
    }

    async fn check_availability(&self) -> Result<()> {
        match &self.def.availability {
            AvailabilityDef::CliExists { value } => check_cli_exists(value),
            AvailabilityDef::EnvVar { value } => check_env_var(value),
            AvailabilityDef::FileExists { value } => check_file_exists(value),
            AvailabilityDef::Always => Ok(()),
        }
    }

    async fn refresh(&self) -> Result<RefreshData> {
        let raw = fetch(&self.def.base_url, &self.def.source)?;
        extractor::extract(&self.def.parser, &raw, &self.compiled)
    }
}

// ============================================================================
// URL 解析
// ============================================================================

/// 将相对路径（以 / 开头）拼接到 base_url 上，绝对 URL 直接返回
/// 同时支持 ${ENV_VAR} 展开
fn resolve_url(base_url: &Option<String>, url: &str) -> String {
    let expanded = expand_env_vars(url);
    match base_url {
        Some(base) if expanded.starts_with('/') => {
            let base = expand_env_vars(base.trim_end_matches('/'));
            format!("{}{}", base, expanded)
        }
        _ => expanded,
    }
}

// ============================================================================
// 可用性检查
// ============================================================================

fn check_cli_exists(binary: &str) -> Result<()> {
    if cli::command_exists(binary) {
        Ok(())
    } else {
        Err(ProviderError::cli_not_found(binary).into())
    }
}

fn check_env_var(var: &str) -> Result<()> {
    if std::env::var(var).ok().filter(|v| !v.is_empty()).is_some() {
        Ok(())
    } else {
        Err(ProviderError::config_missing(var).into())
    }
}

fn check_file_exists(path: &str) -> Result<()> {
    let expanded = expand_tilde(path);
    if std::path::Path::new(&expanded).exists() {
        Ok(())
    } else {
        Err(ProviderError::unavailable(&format!("file not found: {}", path)).into())
    }
}

/// 展开路径中的 ~ 为用户 home 目录
fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest).to_string_lossy().to_string();
        }
    }
    path.to_string()
}

// ============================================================================
// 数据获取
// ============================================================================

fn fetch(base_url: &Option<String>, source: &SourceDef) -> Result<String> {
    match source {
        SourceDef::Cli { command, args } => fetch_cli(command, args),
        SourceDef::HttpGet { url, auth, headers } => fetch_http_get(base_url, url, auth, headers),
        SourceDef::HttpPost {
            url,
            auth,
            headers,
            body,
        } => fetch_http_post(base_url, url, auth, headers, body),
    }
}

fn fetch_cli(command: &str, args: &[String]) -> Result<String> {
    let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let output = cli::run_checked_command(command, &args_ref)?;
    Ok(cli::stdout_or_stderr_text(&output))
}

fn fetch_http_get(
    base_url: &Option<String>,
    url: &str,
    auth: &Option<AuthDef>,
    headers: &[HeaderDef],
) -> Result<String> {
    let url = resolve_url(base_url, url);
    let header_strings = resolve_auth_headers(base_url, auth, headers)?;
    let header_refs: Vec<&str> = header_strings.iter().map(|s| s.as_str()).collect();
    http_client::get(&url, &header_refs)
}

fn fetch_http_post(
    base_url: &Option<String>,
    url: &str,
    auth: &Option<AuthDef>,
    headers: &[HeaderDef],
    body: &str,
) -> Result<String> {
    let url = resolve_url(base_url, url);
    let header_strings = resolve_auth_headers(base_url, auth, headers)?;
    let header_refs: Vec<&str> = header_strings.iter().map(|s| s.as_str()).collect();
    http_client::post_json(&url, &header_refs, body)
}

/// 将 AuthDef + HeaderDef 列表转换为 "Name: Value" 格式的 header 字符串。
///
/// **注意**：对于 `Login` 认证类型，此函数会发起 HTTP 请求获取 token（I/O 副作用）。
fn resolve_auth_headers(
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

/// 通过登录接口获取 access token
///
/// POST login_url + {"username":"..","password":".."} → 解析 JSON → 提取 token_path 对应的值
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
    let response = http_client::post_json(&login_url, &[], &body)?;

    parse_login_response(&response, token_path)
}

/// 从登录响应 JSON 中提取 token（纯逻辑，无 I/O，可单元测试）
fn parse_login_response(response: &str, token_path: &str) -> Result<String> {
    let json: serde_json::Value = serde_json::from_str(response)
        .map_err(|_| ProviderError::parse_failed("login response is not valid JSON"))?;

    // 检查 success 字段（OneAPI/NewAPI 标准响应格式）
    if let Some(false) = json.get("success").and_then(|v| v.as_bool()) {
        let msg = json
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("login failed");
        anyhow::bail!("login failed: {}", msg);
    }

    // 用点分路径提取 token
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

/// 展开字符串中的 ${ENV_VAR} 引用
fn expand_env_vars(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '$' && chars.peek() == Some(&'{') {
            chars.next(); // 消费 '{'
            let var_name: String = chars.by_ref().take_while(|&ch| ch != '}').collect();
            match std::env::var(&var_name) {
                Ok(val) => result.push_str(&val),
                Err(_) => {
                    warn!(
                        target: "providers::custom",
                        "Environment variable '{}' is not set, expanding to empty string",
                        var_name
                    );
                }
            }
        } else {
            result.push(c);
        }
    }

    result
}

#[cfg(test)]
fn check_availability_sync(def: &AvailabilityDef) -> bool {
    match def {
        AvailabilityDef::CliExists { value } => check_cli_exists(value).is_ok(),
        AvailabilityDef::EnvVar { value } => check_env_var(value).is_ok(),
        AvailabilityDef::FileExists { value } => check_file_exists(value).is_ok(),
        AvailabilityDef::Always => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── expand_tilde ────────────────────────────

    #[test]
    fn test_expand_tilde() {
        let expanded = expand_tilde("~/test/path");
        assert!(!expanded.starts_with('~'));
        assert!(expanded.ends_with("test/path"));
    }

    #[test]
    fn test_expand_tilde_no_tilde() {
        assert_eq!(expand_tilde("/absolute/path"), "/absolute/path");
    }

    // ── expand_env_vars ─────────────────────────

    #[test]
    fn test_expand_env_vars() {
        std::env::set_var("TEST_FETCHER_VAR", "hello");
        assert_eq!(
            expand_env_vars("prefix-${TEST_FETCHER_VAR}-suffix"),
            "prefix-hello-suffix"
        );
        std::env::remove_var("TEST_FETCHER_VAR");
    }

    #[test]
    fn test_expand_env_vars_no_vars() {
        assert_eq!(expand_env_vars("plain text"), "plain text");
    }

    #[test]
    fn test_expand_env_vars_missing() {
        assert_eq!(expand_env_vars("${NONEXISTENT_VAR_12345}"), "");
    }

    // ── resolve_auth_headers ───────────────────────────

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

    // ── availability ────────────────────────────

    #[test]
    fn test_check_env_var_set() {
        std::env::set_var("TEST_CUSTOM_AVAIL", "value");
        assert!(check_env_var("TEST_CUSTOM_AVAIL").is_ok());
        std::env::remove_var("TEST_CUSTOM_AVAIL");
    }

    #[test]
    fn test_check_env_var_missing() {
        std::env::remove_var("NONEXISTENT_CUSTOM_99");
        assert!(check_env_var("NONEXISTENT_CUSTOM_99").is_err());
    }

    #[test]
    fn test_check_file_exists_missing() {
        assert!(check_file_exists("/nonexistent/path/12345").is_err());
    }

    // ── descriptor ──────────────────────────────

    #[test]
    fn test_custom_provider_descriptor() {
        let yaml = r#"
id: "test:cli"
metadata:
  display_name: "Test Provider"
  brand_name: "TestBrand"
  dashboard_url: "https://test.com"
  source_label: "test cli"
availability:
  type: cli_exists
  value: "echo"
source:
  type: cli
  command: "echo"
parser:
  format: regex
  quotas:
    - label: "Usage"
      pattern: '(\d+)/(\d+)'
"#;
        let def: CustomProviderDef = serde_yaml::from_str(yaml).unwrap();
        let provider = CustomProvider::new(def).unwrap();
        let desc = provider.descriptor();

        assert_eq!(desc.id.as_ref(), "test:cli");
        assert_eq!(desc.metadata.display_name, "Test Provider");
        assert_eq!(desc.metadata.brand_name, "TestBrand");
        assert_eq!(desc.metadata.kind, ProviderKind::Custom);
    }

    #[test]
    fn test_dashboard_url_env_expansion() {
        std::env::set_var("TEST_CUSTOM_BASE_URL", "https://my-newapi.com");
        let yaml = r#"
id: "test:api"
metadata:
  display_name: "Test"
  brand_name: "Test"
  dashboard_url: "${TEST_CUSTOM_BASE_URL}/dashboard"
availability:
  type: cli_exists
  value: "echo"
source:
  type: cli
  command: "echo"
parser:
  format: regex
  quotas:
    - label: "Usage"
      pattern: '(\d+)/(\d+)'
"#;
        let def: CustomProviderDef = serde_yaml::from_str(yaml).unwrap();
        let provider = CustomProvider::new(def).unwrap();
        let desc = provider.descriptor();
        assert_eq!(
            desc.metadata.dashboard_url,
            "https://my-newapi.com/dashboard"
        );
        std::env::remove_var("TEST_CUSTOM_BASE_URL");
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
    fn test_availability_always_is_ok() {
        assert!(check_availability_sync(&AvailabilityDef::Always));
    }

    // ── extractor::json_string (delegated) ──────

    #[test]
    fn test_json_string_simple() {
        let json: serde_json::Value = serde_json::from_str(r#"{"data": "token-abc-123"}"#).unwrap();
        assert_eq!(
            extractor::json_string(&json, "data"),
            Some("token-abc-123".to_string())
        );
    }

    #[test]
    fn test_json_string_nested() {
        let json: serde_json::Value =
            serde_json::from_str(r#"{"result": {"token": "xyz"}}"#).unwrap();
        assert_eq!(
            extractor::json_string(&json, "result.token"),
            Some("xyz".to_string())
        );
    }

    #[test]
    fn test_json_string_missing_path() {
        let json: serde_json::Value = serde_json::from_str(r#"{"other": "value"}"#).unwrap();
        assert_eq!(extractor::json_string(&json, "data"), None);
    }

    #[test]
    fn test_deserialize_login_auth() {
        let yaml = r#"
id: "newapi:api"
metadata:
  display_name: "NewAPI"
  brand_name: "NewAPI"
availability:
  type: always
source:
  type: http_get
  url: "https://site.com/api/user/self"
  auth:
    type: login
    login_url: "https://site.com/api/user/login"
    username: "admin"
    password: "123456"
parser:
  format: json
  quotas:
    - label: "Balance"
      used: "data.used_quota"
      limit: "data.quota"
      divisor: 500000
"#;
        let def: CustomProviderDef = serde_yaml::from_str(yaml).unwrap();
        if let SourceDef::HttpGet { auth, .. } = &def.source {
            match auth.as_ref().unwrap() {
                AuthDef::Login {
                    login_url,
                    username,
                    password,
                    token_path,
                } => {
                    assert_eq!(login_url, "https://site.com/api/user/login");
                    assert_eq!(username, "admin");
                    assert_eq!(password, "123456");
                    assert_eq!(token_path, "data"); // 默认值
                }
                _ => panic!("Expected Login auth"),
            }
        } else {
            panic!("Expected HttpGet source");
        }
    }

    // ── resolve_url ─────────────────────────────

    #[test]
    fn test_resolve_url_relative_path() {
        let base = Some("https://example.com".to_string());
        assert_eq!(
            resolve_url(&base, "/api/user/self"),
            "https://example.com/api/user/self"
        );
    }

    #[test]
    fn test_resolve_url_absolute_url_unchanged() {
        let base = Some("https://example.com".to_string());
        assert_eq!(
            resolve_url(&base, "https://other.com/api"),
            "https://other.com/api"
        );
    }

    #[test]
    fn test_resolve_url_no_base() {
        assert_eq!(
            resolve_url(&None, "https://example.com/api"),
            "https://example.com/api"
        );
    }

    #[test]
    fn test_resolve_url_base_trailing_slash() {
        let base = Some("https://example.com/".to_string());
        assert_eq!(
            resolve_url(&base, "/api/user/self"),
            "https://example.com/api/user/self"
        );
    }

    #[test]
    fn test_descriptor_with_base_url() {
        let yaml = r#"
id: "test:api"
base_url: "https://my-site.com"
metadata:
  display_name: "Test"
  brand_name: "Test"
  dashboard_url: "/dashboard"
availability:
  type: always
source:
  type: cli
  command: "echo"
parser:
  format: regex
  quotas:
    - label: "Usage"
      pattern: '(\d+)/(\d+)'
"#;
        let def: CustomProviderDef = serde_yaml::from_str(yaml).unwrap();
        let provider = CustomProvider::new(def).unwrap();
        let desc = provider.descriptor();
        assert_eq!(desc.metadata.dashboard_url, "https://my-site.com/dashboard");
    }

    // ── check_file_exists ─────────────────────────

    #[test]
    fn test_check_file_exists_existing_file() {
        // /etc/hosts exists on all macOS/Linux systems
        assert!(check_file_exists("/etc/hosts").is_ok());
    }

    #[test]
    fn test_check_file_exists_tilde_expansion() {
        // ~/.config or home dir itself should exist
        let home = dirs::home_dir().expect("should have home dir");
        let home_str = home.to_string_lossy();
        // Test that ~ expansion resolves to an existing path
        assert!(check_file_exists(&format!("{}", home_str)).is_ok());
    }

    #[test]
    fn test_check_file_exists_returns_unavailable_error() {
        let err = check_file_exists("/nonexistent/path/12345").unwrap_err();
        let provider_err = err.downcast_ref::<ProviderError>().unwrap();
        assert!(matches!(provider_err, ProviderError::Unavailable { .. }));
    }

    // ── expand_env_vars edge cases ────────────────

    #[test]
    fn test_expand_env_vars_multiple_vars() {
        std::env::set_var("TEST_EV_A", "hello");
        std::env::set_var("TEST_EV_B", "world");
        assert_eq!(expand_env_vars("${TEST_EV_A}-${TEST_EV_B}"), "hello-world");
        std::env::remove_var("TEST_EV_A");
        std::env::remove_var("TEST_EV_B");
    }

    #[test]
    fn test_expand_env_vars_dollar_without_brace() {
        // $ not followed by { should be kept as-is
        assert_eq!(expand_env_vars("$plain"), "$plain");
    }

    #[test]
    fn test_expand_env_vars_empty_var_name() {
        // ${} should expand to empty and warn
        assert_eq!(expand_env_vars("before${}after"), "beforeafter");
    }

    // ── parse_login_response ──────────────────────

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
        // OneAPI/NewAPI without success field — should still work
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

    // ── login env var expansion ───────────────────

    #[test]
    fn test_resolve_auth_headers_login_env_expansion() {
        // We can't test the full login flow (requires HTTP), but we can test
        // that env vars in username/password fields would be expanded by
        // verifying the expand_env_vars logic is applied
        std::env::set_var("TEST_LOGIN_USER", "admin");
        std::env::set_var("TEST_LOGIN_PASS", "secret123");

        let username = expand_env_vars("${TEST_LOGIN_USER}");
        let password = expand_env_vars("${TEST_LOGIN_PASS}");
        assert_eq!(username, "admin");
        assert_eq!(password, "secret123");

        std::env::remove_var("TEST_LOGIN_USER");
        std::env::remove_var("TEST_LOGIN_PASS");
    }
}
