use anyhow::Result;
use async_trait::async_trait;
use std::borrow::Cow;

use crate::models::{ProviderDescriptor, ProviderKind, ProviderMetadata, RefreshData};
use crate::providers::common::cli;
use crate::providers::{AiProvider, ProviderError};
use crate::utils::http_client;

use super::extractor;
use super::schema::{AuthDef, AvailabilityDef, CustomProviderDef, HeaderDef, SourceDef};

/// 基于 YAML 定义的自定义 Provider 运行时
pub struct CustomProvider {
    def: CustomProviderDef,
}

impl CustomProvider {
    pub fn new(def: CustomProviderDef) -> Self {
        Self { def }
    }

    pub fn id(&self) -> &str {
        &self.def.id
    }
}

#[async_trait]
impl AiProvider for CustomProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: Cow::Owned(self.def.id.clone()),
            metadata: ProviderMetadata {
                kind: ProviderKind::Custom,
                display_name: self.def.metadata.display_name.clone(),
                brand_name: self.def.metadata.brand_name.clone(),
                icon_asset: self.def.metadata.icon.clone(),
                dashboard_url: self.def.metadata.dashboard_url.clone(),
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
        }
    }

    async fn refresh(&self) -> Result<RefreshData> {
        let raw = fetch(&self.def.source)?;
        extractor::extract(&self.def.parser, &raw)
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
        Err(ProviderError::config_missing(path).into())
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

fn fetch(source: &SourceDef) -> Result<String> {
    match source {
        SourceDef::Cli { command, args } => fetch_cli(command, args),
        SourceDef::HttpGet { url, auth, headers } => fetch_http_get(url, auth, headers),
        SourceDef::HttpPost {
            url,
            auth,
            headers,
            body,
        } => fetch_http_post(url, auth, headers, body),
    }
}

fn fetch_cli(command: &str, args: &[String]) -> Result<String> {
    let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let output = cli::run_checked_command(command, &args_ref)?;
    Ok(cli::stdout_or_stderr_text(&output))
}

fn fetch_http_get(url: &str, auth: &Option<AuthDef>, headers: &[HeaderDef]) -> Result<String> {
    let header_strings = build_headers(auth, headers)?;
    let header_refs: Vec<&str> = header_strings.iter().map(|s| s.as_str()).collect();
    http_client::get(url, &header_refs)
}

fn fetch_http_post(
    url: &str,
    auth: &Option<AuthDef>,
    headers: &[HeaderDef],
    body: &str,
) -> Result<String> {
    let header_strings = build_headers(auth, headers)?;
    let header_refs: Vec<&str> = header_strings.iter().map(|s| s.as_str()).collect();
    http_client::post_json(url, &header_refs, body)
}

/// 将 AuthDef + HeaderDef 列表转换为 "Name: Value" 格式的 header 字符串
fn build_headers(auth: &Option<AuthDef>, headers: &[HeaderDef]) -> Result<Vec<String>> {
    let mut result = Vec::new();

    if let Some(auth) = auth {
        match auth {
            AuthDef::BearerEnv { env_var } => {
                let token = read_env(env_var)?;
                result.push(format!("Authorization: Bearer {}", token));
            }
            AuthDef::HeaderEnv { header, env_var } => {
                let value = read_env(env_var)?;
                result.push(format!("{}: {}", header, value));
            }
        }
    }

    for h in headers {
        let value = expand_env_vars(&h.value);
        result.push(format!("{}: {}", h.name, value));
    }

    Ok(result)
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
            if let Ok(val) = std::env::var(&var_name) {
                result.push_str(&val);
            }
        } else {
            result.push(c);
        }
    }

    result
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

    // ── build_headers ───────────────────────────

    #[test]
    fn test_build_headers_bearer_env() {
        std::env::set_var("TEST_TOKEN_FETCHER", "tok123");
        let auth = Some(AuthDef::BearerEnv {
            env_var: "TEST_TOKEN_FETCHER".to_string(),
        });
        let headers = build_headers(&auth, &[]).unwrap();
        assert_eq!(headers, vec!["Authorization: Bearer tok123"]);
        std::env::remove_var("TEST_TOKEN_FETCHER");
    }

    #[test]
    fn test_build_headers_missing_env() {
        std::env::remove_var("MISSING_TOKEN_12345");
        let auth = Some(AuthDef::BearerEnv {
            env_var: "MISSING_TOKEN_12345".to_string(),
        });
        assert!(build_headers(&auth, &[]).is_err());
    }

    #[test]
    fn test_build_headers_custom_with_env_expansion() {
        std::env::set_var("TEST_CUSTOM_HEADER_VAL", "secret");
        let headers = vec![HeaderDef {
            name: "X-Custom".to_string(),
            value: "Bearer ${TEST_CUSTOM_HEADER_VAL}".to_string(),
        }];
        let result = build_headers(&None, &headers).unwrap();
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
        let provider = CustomProvider::new(def);
        let desc = provider.descriptor();

        assert_eq!(desc.id.as_ref(), "test:cli");
        assert_eq!(desc.metadata.display_name, "Test Provider");
        assert_eq!(desc.metadata.brand_name, "TestBrand");
        assert_eq!(desc.metadata.kind, ProviderKind::Custom);
    }
}
