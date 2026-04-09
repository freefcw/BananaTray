use anyhow::Result;
use async_trait::async_trait;
use log::{debug, info, warn};
use std::borrow::Cow;

use crate::models::{ProviderDescriptor, ProviderKind, ProviderMetadata, RefreshData};
use crate::providers::common::cli;
use crate::providers::{AiProvider, ProviderError};
use crate::utils::http_client;

use super::extractor::{self, CompiledPatterns};
use super::schema::{
    AuthDef, AvailabilityDef, CustomProviderDef, HeaderDef, PreprocessStep, SourceDef,
};

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
        // 当 icon 为空（默认）时，从 display_name 取首字母作为单色图标
        let icon_asset = if self.def.metadata.icon.is_empty() {
            first_letter_icon(&self.def.metadata.display_name)
        } else {
            self.def.metadata.icon.clone()
        };
        ProviderDescriptor {
            id: Cow::Owned(self.def.id.clone()),
            metadata: ProviderMetadata {
                kind: ProviderKind::Custom,
                display_name: self.def.metadata.display_name.clone(),
                brand_name: self.def.metadata.brand_name.clone(),
                icon_asset,
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
            AvailabilityDef::FileJsonMatch {
                path,
                json_path,
                expected,
            } => check_file_json_match(path, json_path, expected),
            AvailabilityDef::DirContains { path, prefix } => check_dir_contains(path, prefix),
            AvailabilityDef::Always => Ok(()),
        }
    }

    async fn refresh(&self) -> Result<RefreshData> {
        let id = &self.def.id;
        info!(target: "providers::custom", "[{}] refresh started", id);

        let raw = fetch(id, &self.def.base_url, &self.def.source)?;

        debug!(target: "providers::custom", "[{}] raw response ({} bytes): {}", id, raw.len(), truncate_for_log(&raw, 500));

        let raw = apply_preprocess(&raw, &self.def.preprocess);

        let parser = self.def.parser.as_ref().ok_or_else(|| {
            warn!(target: "providers::custom", "[{}] no parser configured", id);
            ProviderError::unavailable("no parser configured (placeholder provider)")
        })?;

        let result = extractor::extract(parser, &raw, &self.compiled);
        match &result {
            Ok(data) => info!(
                target: "providers::custom",
                "[{}] parsed {} quotas, email={:?}",
                id, data.quotas.len(), data.account_email
            ),
            Err(e) => warn!(
                target: "providers::custom",
                "[{}] parse failed: {}\n  raw response: {}",
                id, e, truncate_for_log(&raw, 300)
            ),
        }
        result
    }
}

// ============================================================================
// 日志辅助
// ============================================================================

/// 截断长文本用于日志输出，避免日志爆炸
///
/// 使用 char_indices 确保截断在字符边界上，避免多字节 UTF-8 切割 panic
fn truncate_for_log(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        // 找到 <= max_len 的最后一个字符边界
        let safe_end = s
            .char_indices()
            .take_while(|(i, _)| *i <= max_len)
            .last()
            .map(|(i, _)| i)
            .unwrap_or(0);
        format!("{}...(truncated, total {} bytes)", &s[..safe_end], s.len())
    }
}

/// 脱敏 auth 头信息：只显示 value 的前几个字符
fn mask_auth_header(header: &str) -> String {
    const VISIBLE_LEN: usize = 8;
    if let Some((name, value)) = header.split_once(':') {
        let value = value.trim();
        let masked = if value.len() > VISIBLE_LEN {
            // 找到安全的字符边界
            let safe_end = value
                .char_indices()
                .take_while(|(i, _)| *i <= VISIBLE_LEN)
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
            format!("{}...", &value[..safe_end])
        } else {
            value.to_string()
        };
        format!("{}: {}", name.trim(), masked)
    } else {
        header.to_string()
    }
}

// ============================================================================
// 图标生成
// ============================================================================

/// 从 display_name 提取首字母（大写）作为单色图标文本
///
/// 中文取第一个汉字，英文取首字母大写。
/// 例：\"NewAPI\" → \"N\"，\"月之暗面\" → \"月\"
fn first_letter_icon(display_name: &str) -> String {
    display_name
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string())
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

fn check_file_json_match(path: &str, json_path: &str, expected: &str) -> Result<()> {
    let json = read_json_file(path)?;
    let actual = extractor::json_navigate(&json, json_path)
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if actual == expected {
        Ok(())
    } else {
        Err(ProviderError::config_missing(&format!(
            "{}:{} (expected '{}', got '{}')",
            path, json_path, expected, actual
        ))
        .into())
    }
}

fn check_dir_contains(path: &str, prefix: &str) -> Result<()> {
    let expanded = expand_tilde(path);
    if let Ok(entries) = std::fs::read_dir(&expanded) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.starts_with(prefix) {
                    return Ok(());
                }
            }
        }
    }
    Err(
        ProviderError::unavailable(&format!("no entry with prefix '{}' in {}", prefix, path))
            .into(),
    )
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

/// 应用预处理管道
fn apply_preprocess(raw: &str, steps: &[PreprocessStep]) -> String {
    if steps.is_empty() {
        return raw.to_string();
    }
    let mut result = raw.to_string();
    for step in steps {
        match step {
            PreprocessStep::StripAnsi => {
                result = crate::utils::text_utils::strip_terminal_noise(&result);
            }
        }
    }
    result
}

fn fetch(id: &str, base_url: &Option<String>, source: &SourceDef) -> Result<String> {
    match source {
        SourceDef::Cli { command, args } => {
            debug!(target: "providers::custom", "[{}] fetching via CLI: {} {:?}", id, command, args);
            fetch_cli(command, args)
        }
        SourceDef::HttpGet { url, auth, headers } => {
            let resolved = resolve_url(base_url, url);
            debug!(target: "providers::custom", "[{}] fetching via HTTP GET: {}", id, resolved);
            let result = fetch_http_get(base_url, url, auth, headers);
            if let Err(ref e) = result {
                warn!(target: "providers::custom", "[{}] HTTP GET failed: {}", id, e);
            }
            result
        }
        SourceDef::HttpPost {
            url,
            auth,
            headers,
            body,
        } => {
            let resolved = resolve_url(base_url, url);
            debug!(target: "providers::custom", "[{}] fetching via HTTP POST: {} (body {} bytes)", id, resolved, body.len());
            let result = fetch_http_post(base_url, url, auth, headers, body);
            if let Err(ref e) = result {
                warn!(target: "providers::custom", "[{}] HTTP POST failed: {}", id, e);
            }
            result
        }
        SourceDef::Placeholder { reason } => {
            debug!(target: "providers::custom", "[{}] placeholder source, reason: {}", id, reason);
            Err(ProviderError::unavailable(reason).into())
        }
    }
}

fn fetch_cli(command: &str, args: &[String]) -> Result<String> {
    let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    cli::run_lenient_command(command, &args_ref)
}

fn fetch_http_get(
    base_url: &Option<String>,
    url: &str,
    auth: &Option<AuthDef>,
    headers: &[HeaderDef],
) -> Result<String> {
    let url = resolve_url(base_url, url);
    let header_strings = resolve_auth_headers(base_url, auth, headers)?;
    debug!(
        target: "providers::custom",
        "request headers ({}): {:?}",
        header_strings.len(),
        header_strings.iter().map(|h| mask_auth_header(h)).collect::<Vec<_>>()
    );
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
    debug!(
        target: "providers::custom",
        "request headers ({}): {:?}",
        header_strings.len(),
        header_strings.iter().map(|h| mask_auth_header(h)).collect::<Vec<_>>()
    );
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

/// 从本地 JSON 文件中读取 token（纯 I/O + JSON 点分路径提取）
fn read_file_token(path: &str, token_path: &str) -> Result<String> {
    let json = read_json_file(path)?;
    extractor::json_string(&json, token_path).ok_or_else(|| {
        ProviderError::config_missing(&format!("token not found at '{}' in {}", token_path, path))
            .into()
    })
}

/// 读取本地 JSON 文件并解析，公共基础设施
fn read_json_file(path: &str) -> Result<serde_json::Value> {
    let expanded = expand_tilde(path);
    let content = std::fs::read_to_string(&expanded)
        .map_err(|_| ProviderError::unavailable(&format!("file not found: {}", path)))?;
    serde_json::from_str(&content)
        .map_err(|_| ProviderError::parse_failed(&format!("invalid JSON in: {}", path)).into())
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
        AvailabilityDef::FileJsonMatch {
            path,
            json_path,
            expected,
        } => check_file_json_match(path, json_path, expected).is_ok(),
        AvailabilityDef::DirContains { path, prefix } => check_dir_contains(path, prefix).is_ok(),
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
        // 未指定 icon → 自动从 display_name 取首字母
        assert_eq!(desc.metadata.icon_asset, "T");
    }

    // ── first_letter_icon ────────────────────────

    #[test]
    fn test_first_letter_icon_english() {
        assert_eq!(first_letter_icon("NewAPI"), "N");
    }

    #[test]
    fn test_first_letter_icon_lowercase() {
        assert_eq!(first_letter_icon("myProvider"), "M");
    }

    #[test]
    fn test_first_letter_icon_chinese() {
        assert_eq!(first_letter_icon("月之暗面"), "月");
    }

    #[test]
    fn test_first_letter_icon_empty() {
        assert_eq!(first_letter_icon(""), "?");
    }

    #[test]
    fn test_descriptor_explicit_icon_preserved() {
        let yaml = r#"
id: "test:cli"
metadata:
  display_name: "Test Provider"
  brand_name: "TestBrand"
  icon: "X"
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
        // 显式指定 icon → 保留原值
        assert_eq!(desc.metadata.icon_asset, "X");
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

    // ── Phase 3: placeholder source ──────────────

    #[test]
    fn test_placeholder_source_returns_unavailable() {
        let result = fetch(
            "test:placeholder",
            &None,
            &SourceDef::Placeholder {
                reason: "No public API available".to_string(),
            },
        );
        let err = result.unwrap_err();
        assert!(err.to_string().contains("No public API available"));
    }

    // ── Phase 3: file_json_match availability ────

    #[test]
    fn test_file_json_match_success() {
        let dir = tempfile::tempdir().unwrap();
        let json_path = dir.path().join("settings.json");
        std::fs::write(
            &json_path,
            r#"{"security":{"auth":{"selectedType":"vertex-ai"}}}"#,
        )
        .unwrap();
        let result = check_file_json_match(
            json_path.to_str().unwrap(),
            "security.auth.selectedType",
            "vertex-ai",
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_file_json_match_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let json_path = dir.path().join("settings.json");
        std::fs::write(
            &json_path,
            r#"{"security":{"auth":{"selectedType":"gemini"}}}"#,
        )
        .unwrap();
        let result = check_file_json_match(
            json_path.to_str().unwrap(),
            "security.auth.selectedType",
            "vertex-ai",
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("expected 'vertex-ai'"));
        assert!(err.to_string().contains("got 'gemini'"));
    }

    #[test]
    fn test_file_json_match_file_not_found() {
        let result = check_file_json_match("/nonexistent/path/settings.json", "some.path", "value");
        assert!(result.is_err());
    }

    #[test]
    fn test_file_json_match_invalid_json() {
        let dir = tempfile::tempdir().unwrap();
        let json_path = dir.path().join("bad.json");
        std::fs::write(&json_path, "not json").unwrap();
        let result = check_file_json_match(json_path.to_str().unwrap(), "some.path", "value");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("invalid JSON"));
    }

    // ── Phase 3: dir_contains availability ───────

    #[test]
    fn test_dir_contains_success() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("kilocode.kilo-code-1.0.0")).unwrap();
        let result = check_dir_contains(dir.path().to_str().unwrap(), "kilocode.kilo-code");
        assert!(result.is_ok());
    }

    #[test]
    fn test_dir_contains_no_match() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("other-extension-1.0")).unwrap();
        let result = check_dir_contains(dir.path().to_str().unwrap(), "kilocode.kilo-code");
        assert!(result.is_err());
    }

    #[test]
    fn test_dir_contains_nonexistent_dir() {
        let result = check_dir_contains("/nonexistent/path/12345", "some-prefix");
        assert!(result.is_err());
    }

    // ── Phase 3: file_token auth ─────────────────

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

    // ── Phase 3: preprocess strip_ansi ───────────

    #[test]
    fn test_apply_preprocess_empty_steps() {
        let raw = "hello \x1b[32mworld\x1b[0m";
        let result = apply_preprocess(raw, &[]);
        // 没有预处理步骤时，原样返回
        assert_eq!(result, raw);
    }

    #[test]
    fn test_apply_preprocess_strip_ansi() {
        use super::PreprocessStep;
        let raw = "Usage: \x1b[1m\x1b[32m25\x1b[0m / \x1b[1m100\x1b[0m requests";
        let result = apply_preprocess(raw, &[PreprocessStep::StripAnsi]);
        assert_eq!(result, "Usage: 25 / 100 requests");
    }

    #[test]
    fn test_apply_preprocess_strip_ansi_with_progress_chars() {
        use super::PreprocessStep;
        // 模拟 Kiro CLI 输出中的进度条字符
        let raw = "⣾⣽⣻ Loading...\x1b[2K\x1b[1AUsage: 10/50\n";
        let result = apply_preprocess(raw, &[PreprocessStep::StripAnsi]);
        assert!(result.contains("Usage: 10/50"));
        assert!(!result.contains("\x1b["));
    }

    // ── Phase 3: check_availability_sync for new types ──

    #[test]
    fn test_availability_sync_file_json_match() {
        let dir = tempfile::tempdir().unwrap();
        let json_path = dir.path().join("config.json");
        std::fs::write(&json_path, r#"{"mode":"enabled"}"#).unwrap();

        let def = AvailabilityDef::FileJsonMatch {
            path: json_path.to_str().unwrap().to_string(),
            json_path: "mode".to_string(),
            expected: "enabled".to_string(),
        };
        assert!(check_availability_sync(&def));

        let def_mismatch = AvailabilityDef::FileJsonMatch {
            path: json_path.to_str().unwrap().to_string(),
            json_path: "mode".to_string(),
            expected: "disabled".to_string(),
        };
        assert!(!check_availability_sync(&def_mismatch));
    }

    #[test]
    fn test_availability_sync_dir_contains() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("my-extension-v1")).unwrap();

        let def = AvailabilityDef::DirContains {
            path: dir.path().to_str().unwrap().to_string(),
            prefix: "my-extension".to_string(),
        };
        assert!(check_availability_sync(&def));

        let def_miss = AvailabilityDef::DirContains {
            path: dir.path().to_str().unwrap().to_string(),
            prefix: "other-extension".to_string(),
        };
        assert!(!check_availability_sync(&def_miss));
    }

    // ── truncate_for_log ────────────────────────

    #[test]
    fn test_truncate_short_string_unchanged() {
        assert_eq!(truncate_for_log("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_exact_length_unchanged() {
        assert_eq!(truncate_for_log("12345", 5), "12345");
    }

    #[test]
    fn test_truncate_long_ascii() {
        let result = truncate_for_log("abcdefghij", 5);
        assert!(result.starts_with("abcde"));
        assert!(result.contains("truncated"));
        assert!(result.contains("10 bytes"));
    }

    #[test]
    fn test_truncate_multibyte_no_panic() {
        // "你好世界" = 4 chars, 12 bytes; 截断在 5 字节处应安全
        let s = "你好世界";
        let result = truncate_for_log(s, 5);
        // "你" = 3 bytes, "好" starts at 3, ends at 6
        // 5 字节位置在 "好" 字符中间，应回退到 byte 3（"你"之后）
        assert!(result.starts_with("你"));
        assert!(result.contains("truncated"));
    }

    #[test]
    fn test_truncate_empty_string() {
        assert_eq!(truncate_for_log("", 10), "");
    }

    // ── mask_auth_header ────────────────────────

    #[test]
    fn test_mask_short_value_unchanged() {
        assert_eq!(mask_auth_header("X-Key: abc"), "X-Key: abc");
    }

    #[test]
    fn test_mask_long_value_truncated() {
        let result = mask_auth_header("Authorization: Bearer sk-very-long-token-123");
        assert_eq!(result, "Authorization: Bearer s...");
    }

    #[test]
    fn test_mask_no_colon() {
        assert_eq!(mask_auth_header("no-colon-header"), "no-colon-header");
    }

    #[test]
    fn test_mask_multibyte_no_panic() {
        // value 为中文字符，确保不在多字节中间切割
        let result = mask_auth_header("Cookie: 这是一个很长的中文值用于测试");
        assert!(result.starts_with("Cookie:"));
        assert!(result.contains("..."));
    }
}
