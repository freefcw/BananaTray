use anyhow::Result;
use log::{info, warn};
use regex::Regex;

use crate::models::ProviderKind;
use std::path::{Path, PathBuf};

use super::provider::CustomProvider;
use super::schema::{
    AuthDef, CustomProviderDef, HeaderDef, HttpMethodDef, ParserDef, PlanStepDef, RegexQuotaRule,
    SourceDef,
};

/// 自定义 Provider YAML 文件的搜索目录
fn providers_dir() -> PathBuf {
    crate::platform::paths::custom_providers_dir()
}

/// 扫描默认配置目录，加载所有有效的自定义 Provider 定义
pub fn load_custom_providers() -> Vec<CustomProvider> {
    load_from_dir(&providers_dir())
}

/// 从指定目录加载自定义 Provider（可测试入口）
pub fn load_from_dir(dir: &Path) -> Vec<CustomProvider> {
    if !dir.exists() {
        info!(target: "providers::custom", "Custom providers dir not found: {}", dir.display());
        return Vec::new();
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(err) => {
            warn!(target: "providers::custom", "Failed to read custom providers dir: {}", err);
            return Vec::new();
        }
    };

    // 收集并排序，确保加载顺序确定
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .is_some_and(|ext| ext == "yaml" || ext == "yml")
        })
        .collect();
    paths.sort();

    let mut providers = Vec::new();

    for path in &paths {
        match load_one(path) {
            Ok(provider) => {
                info!(
                    target: "providers::custom",
                    "Loaded custom provider: {} from {}",
                    provider.id(),
                    path.display()
                );
                providers.push(provider);
            }
            Err(err) => {
                warn!(
                    target: "providers::custom",
                    "Failed to load {}: {}",
                    path.display(),
                    err
                );
            }
        }
    }

    info!(
        target: "providers::custom",
        "Loaded {} custom provider(s) from {}",
        providers.len(),
        dir.display()
    );

    providers
}

fn load_one(path: &Path) -> Result<CustomProvider> {
    let content = std::fs::read_to_string(path)?;
    let def: CustomProviderDef = serde_norway::from_str(&content)
        .map_err(|err| augment_legacy_schema_hint(err, &content))?;
    validate(&def)?;
    CustomProvider::new(def)
}

/// 当 deserialize 失败且 YAML 看起来像旧 schema 时，在错误后追加迁移脚本提示。
///
/// `deny_unknown_fields` 让旧 YAML 顶层 `availability/source/parser` 在 deserialize
/// 阶段就死于 `unknown field`，会丢掉 validate 阶段对 `schema_version` 的友好提示，
/// 这里把同等提示补回来。
fn augment_legacy_schema_hint(err: serde_norway::Error, content: &str) -> anyhow::Error {
    if looks_like_legacy_schema(content) {
        anyhow::anyhow!(
            "{err}; YAML appears to use the legacy schema (top-level source/parser), \
             run scripts/migrate_custom_provider_yaml.py to migrate to schema_version 2"
        )
    } else {
        anyhow::Error::from(err)
    }
}

fn looks_like_legacy_schema(content: &str) -> bool {
    let has_schema_version = content
        .lines()
        .any(|line| line.trim_start().starts_with("schema_version:"));
    if has_schema_version {
        return false;
    }
    content
        .lines()
        .any(|line| line.starts_with("source:") || line.starts_with("availability:"))
}

/// 校验定义的合法性，在加载时 fail-fast
fn validate(def: &CustomProviderDef) -> Result<()> {
    if def.schema_version != 2 {
        anyhow::bail!(
            "'schema_version' is {} but must be 2; run scripts/migrate_custom_provider_yaml.py for legacy YAML",
            def.schema_version
        );
    }
    if def.id.trim().is_empty() {
        anyhow::bail!("'id' cannot be empty");
    }
    if ProviderKind::from_id_key(&def.id).is_some() {
        anyhow::bail!(
            "custom provider id '{}' is reserved by a built-in provider; choose a unique id",
            def.id
        );
    }
    if def.metadata.display_name.is_empty() {
        anyhow::bail!("'metadata.display_name' cannot be empty");
    }

    if def.plan.steps.is_empty() {
        anyhow::bail!("'plan.steps' must contain at least one step");
    }
    for step in &def.plan.steps {
        validate_step(step)?;
    }

    Ok(())
}

fn validate_step(step: &PlanStepDef) -> Result<()> {
    if step.name.trim().is_empty() {
        anyhow::bail!("plan step name cannot be empty");
    }
    validate_source(&step.source)?;
    if !matches!(step.source, SourceDef::Placeholder { .. }) && step.parser.is_none() {
        anyhow::bail!(
            "plan step '{}': 'parser' is required unless source.type is placeholder",
            step.name
        );
    }
    validate_parser(&step.parser)?;
    Ok(())
}

fn validate_source(source: &SourceDef) -> Result<()> {
    match source {
        SourceDef::Cli { command, .. } => {
            if command.is_empty() {
                anyhow::bail!("'source.command' cannot be empty");
            }
        }
        SourceDef::Http {
            method,
            url,
            auth,
            headers,
            body,
            ..
        } => {
            if url.is_empty() {
                anyhow::bail!("'source.url' cannot be empty");
            }
            if *method == HttpMethodDef::Post && body.as_ref().is_none_or(|body| body.is_empty()) {
                anyhow::bail!("'source.body' cannot be empty for HTTP POST");
            }
            validate_auth_header(auth)?;
            validate_headers(headers)?;
        }
        SourceDef::Placeholder { reason } => {
            if reason.is_empty() {
                anyhow::bail!("'source.reason' cannot be empty for placeholder provider");
            }
        }
    }
    Ok(())
}

fn validate_auth_header(auth: &Option<AuthDef>) -> Result<()> {
    if let Some(AuthDef::HeaderEnv { header, .. }) = auth {
        validate_header_name(header, "source.auth.header")?;
    }
    Ok(())
}

fn validate_headers(headers: &[HeaderDef]) -> Result<()> {
    for header in headers {
        validate_header_name(&header.name, "source.headers[].name")?;
        ureq::http::HeaderValue::try_from(header.value.as_str()).map_err(|err| {
            anyhow::anyhow!("invalid header value for '{}': {}", header.name, err)
        })?;
    }
    Ok(())
}

fn validate_header_name(name: &str, field: &str) -> Result<()> {
    ureq::http::HeaderName::try_from(name)
        .map(|_| ())
        .map_err(|err| anyhow::anyhow!("invalid header name '{}' in {}: {}", name, field, err))
}

fn validate_parser(parser: &Option<ParserDef>) -> Result<()> {
    let Some(parser) = parser else {
        // parser 可以为 None（placeholder source）
        return Ok(());
    };
    match parser {
        ParserDef::Json { quotas, .. } => {
            if quotas.is_empty() {
                anyhow::bail!("'parser.quotas' must contain at least one rule");
            }
            for rule in quotas {
                // 校验模式互斥：remaining 模式 vs 传统 used+limit 模式
                let has_remaining = rule.remaining.as_ref().is_some_and(|s| !s.is_empty());
                let has_limit = rule.limit.as_ref().is_some_and(|s| !s.is_empty());
                let has_used = rule.used.as_ref().is_some_and(|s| !s.is_empty());

                if has_remaining && has_limit {
                    anyhow::bail!(
                        "quota rule '{}': 'remaining' and 'limit' are mutually exclusive",
                        rule.label
                    );
                }
                if !has_remaining && !has_limit {
                    anyhow::bail!(
                        "quota rule '{}': must specify either 'remaining' or 'used'+'limit'",
                        rule.label
                    );
                }
                if has_limit && !has_used {
                    anyhow::bail!(
                        "quota rule '{}': 'used' is required when 'limit' is specified",
                        rule.label
                    );
                }

                validate_divisor(&rule.label, rule.divisor)?;
            }
        }
        ParserDef::Regex { quotas, .. } => {
            if quotas.is_empty() {
                anyhow::bail!("'parser.quotas' must contain at least one rule");
            }
            for rule in quotas {
                validate_regex_rule(rule)?;
                validate_divisor(&rule.label, rule.divisor)?;
            }
        }
    }
    Ok(())
}

fn validate_regex_rule(rule: &RegexQuotaRule) -> Result<()> {
    let re = Regex::new(&rule.pattern).map_err(|e| {
        anyhow::anyhow!(
            "quota rule '{}': invalid regex '{}': {}",
            rule.label,
            rule.pattern,
            e
        )
    })?;

    let capture_count = re.captures_len() - 1; // group 0 是整个匹配
    if rule.used_group > capture_count {
        anyhow::bail!(
            "quota rule '{}': used_group {} exceeds capture groups ({})",
            rule.label,
            rule.used_group,
            capture_count
        );
    }
    if rule.limit_group > capture_count {
        anyhow::bail!(
            "quota rule '{}': limit_group {} exceeds capture groups ({})",
            rule.label,
            rule.limit_group,
            capture_count
        );
    }

    Ok(())
}

/// 校验 divisor 必须为正数
fn validate_divisor(label: &str, divisor: Option<f64>) -> Result<()> {
    if let Some(d) = divisor {
        if d <= 0.0 {
            anyhow::bail!(
                "quota rule '{}': divisor must be positive, got {}",
                label,
                d
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::custom::schema::*;
    use std::fs;

    fn make_minimal_def() -> CustomProviderDef {
        CustomProviderDef {
            schema_version: 2,
            id: "test:cli".to_string(),
            base_url: None,
            metadata: MetadataDef {
                display_name: "Test".to_string(),
                brand_name: "Test".to_string(),
                icon: String::new(),
                dashboard_url: String::new(),
                account_hint: "account".to_string(),
                source_label: String::new(),
            },
            plan: PlanDef {
                mode: PlanMode::FirstSuccess,
                steps: vec![PlanStepDef {
                    name: "default".to_string(),
                    required: true,
                    availability: Some(AvailabilityDef::CliExists {
                        value: "echo".to_string(),
                    }),
                    source: SourceDef::Cli {
                        command: "echo".to_string(),
                        args: vec![],
                        timeout_ms: None,
                    },
                    parser: Some(ParserDef::Regex {
                        account_email: None,
                        quotas: vec![RegexQuotaRule {
                            label: "Usage".to_string(),
                            pattern: r"(\d+)/(\d+)".to_string(),
                            used_group: 1,
                            limit_group: 2,
                            quota_type: QuotaTypeDef::General,
                            divisor: None,
                        }],
                    }),
                    preprocess: vec![],
                }],
            },
        }
    }

    fn step_mut(def: &mut CustomProviderDef) -> &mut PlanStepDef {
        def.plan.steps.first_mut().unwrap()
    }

    // ── validate ────────────────────────────────

    #[test]
    fn test_validate_valid() {
        assert!(validate(&make_minimal_def()).is_ok());
    }

    #[test]
    fn test_validate_rejects_legacy_schema() {
        let mut def = make_minimal_def();
        def.schema_version = 1;
        let err = validate(&def).unwrap_err();
        assert!(err.to_string().contains("schema_version"));
        // 验证错误信息包含实际值
        assert!(err.to_string().contains(" is 1 "));
    }

    #[test]
    fn test_validate_empty_id() {
        let mut def = make_minimal_def();
        def.id = String::new();
        assert!(validate(&def).is_err());
    }

    #[test]
    fn test_validate_rejects_builtin_provider_id() {
        for reserved in ["claude", "codex", "gemini"] {
            let mut def = make_minimal_def();
            def.id = reserved.to_string();
            let error = validate(&def).unwrap_err().to_string();
            assert!(error.contains("reserved"), "unexpected error: {error}");
            assert!(error.contains(reserved));
        }
    }

    #[test]
    fn test_validate_empty_display_name() {
        let mut def = make_minimal_def();
        def.metadata.display_name = String::new();
        assert!(validate(&def).is_err());
    }

    #[test]
    fn test_validate_empty_steps() {
        let mut def = make_minimal_def();
        def.plan.steps.clear();
        assert!(validate(&def).is_err());
    }

    #[test]
    fn test_validate_empty_step_name() {
        let mut def = make_minimal_def();
        step_mut(&mut def).name = String::new();
        assert!(validate(&def).is_err());
    }

    #[test]
    fn test_validate_empty_command() {
        let mut def = make_minimal_def();
        step_mut(&mut def).source = SourceDef::Cli {
            command: String::new(),
            args: vec![],
            timeout_ms: None,
        };
        assert!(validate(&def).is_err());
    }

    #[test]
    fn test_validate_empty_url() {
        let mut def = make_minimal_def();
        step_mut(&mut def).source = SourceDef::Http {
            method: HttpMethodDef::Get,
            url: String::new(),
            timeout_ms: None,
            auth: None,
            headers: vec![],
            body: None,
        };
        assert!(validate(&def).is_err());
    }

    #[test]
    fn test_validate_post_requires_body() {
        let mut def = make_minimal_def();
        step_mut(&mut def).source = SourceDef::Http {
            method: HttpMethodDef::Post,
            url: "https://example.com/api".to_string(),
            timeout_ms: None,
            auth: None,
            headers: vec![],
            body: None,
        };
        let err = validate(&def).unwrap_err();
        assert!(err.to_string().contains("source.body"));
    }

    #[test]
    fn test_validate_accepts_valid_custom_headers() {
        let mut def = make_minimal_def();
        step_mut(&mut def).source = SourceDef::Http {
            method: HttpMethodDef::Get,
            url: "https://example.com/api".to_string(),
            timeout_ms: None,
            auth: None,
            headers: vec![HeaderDef {
                name: "X-Account-Id".to_string(),
                value: "tenant:primary ${CUSTOM_SUFFIX}".to_string(),
            }],
            body: None,
        };

        assert!(validate(&def).is_ok());
    }

    #[test]
    fn test_validate_rejects_invalid_custom_header_name() {
        let mut def = make_minimal_def();
        step_mut(&mut def).source = SourceDef::Http {
            method: HttpMethodDef::Get,
            url: "https://example.com/api".to_string(),
            timeout_ms: None,
            auth: None,
            headers: vec![HeaderDef {
                name: "X Account".to_string(),
                value: "primary".to_string(),
            }],
            body: None,
        };

        let err = validate(&def).unwrap_err();

        assert!(err.to_string().contains("header name"));
        assert!(err.to_string().contains("X Account"));
    }

    #[test]
    fn test_validate_rejects_invalid_auth_header_name() {
        let mut def = make_minimal_def();
        step_mut(&mut def).source = SourceDef::Http {
            method: HttpMethodDef::Get,
            url: "https://example.com/api".to_string(),
            timeout_ms: None,
            auth: Some(AuthDef::HeaderEnv {
                header: "X Account".to_string(),
                env_var: "CUSTOM_TOKEN".to_string(),
            }),
            headers: vec![],
            body: None,
        };

        let err = validate(&def).unwrap_err();

        assert!(err.to_string().contains("source.auth.header"));
        assert!(err.to_string().contains("X Account"));
    }

    #[test]
    fn test_validate_rejects_custom_header_value_with_newline() {
        let mut def = make_minimal_def();
        step_mut(&mut def).source = SourceDef::Http {
            method: HttpMethodDef::Get,
            url: "https://example.com/api".to_string(),
            timeout_ms: None,
            auth: None,
            headers: vec![HeaderDef {
                name: "X-Account".to_string(),
                value: "primary\r\nX-Injected: true".to_string(),
            }],
            body: None,
        };

        let err = validate(&def).unwrap_err();

        assert!(err.to_string().contains("header value"));
        assert!(err.to_string().contains("X-Account"));
    }

    #[test]
    fn test_validate_empty_quotas() {
        let mut def = make_minimal_def();
        step_mut(&mut def).parser = Some(ParserDef::Regex {
            account_email: None,
            quotas: vec![],
        });
        assert!(validate(&def).is_err());
    }

    #[test]
    fn test_validate_invalid_regex() {
        let mut def = make_minimal_def();
        step_mut(&mut def).parser = Some(ParserDef::Regex {
            account_email: None,
            quotas: vec![RegexQuotaRule {
                label: "Bad".to_string(),
                pattern: "[invalid".to_string(),
                used_group: 1,
                limit_group: 2,
                quota_type: QuotaTypeDef::General,
                divisor: None,
            }],
        });
        let err = validate(&def).unwrap_err();
        assert!(err.to_string().contains("invalid regex"));
    }

    #[test]
    fn test_validate_bad_capture_group() {
        let mut def = make_minimal_def();
        step_mut(&mut def).parser = Some(ParserDef::Regex {
            account_email: None,
            quotas: vec![RegexQuotaRule {
                label: "Bad".to_string(),
                pattern: r"(\d+)".to_string(), // 只有 1 个 group
                used_group: 1,
                limit_group: 5, // 超出
                quota_type: QuotaTypeDef::General,
                divisor: None,
            }],
        });
        let err = validate(&def).unwrap_err();
        assert!(err.to_string().contains("limit_group 5"));
    }

    #[test]
    fn test_validate_empty_json_paths() {
        let mut def = make_minimal_def();
        step_mut(&mut def).parser = Some(ParserDef::Json {
            account_email: None,
            account_tier: None,
            quotas: vec![JsonQuotaRule {
                label: "Test".to_string(),
                used: None,
                limit: None,
                remaining: None,
                quota_type: QuotaTypeDef::General,
                detail: None,
                divisor: None,
            }],
        });
        assert!(validate(&def).is_err());
    }

    #[test]
    fn test_validate_json_remaining_mode_valid() {
        let mut def = make_minimal_def();
        step_mut(&mut def).parser = Some(ParserDef::Json {
            account_email: None,
            account_tier: None,
            quotas: vec![JsonQuotaRule {
                label: "Balance".to_string(),
                used: Some("data.used".to_string()),
                limit: None,
                remaining: Some("data.remaining".to_string()),
                quota_type: QuotaTypeDef::Credit,
                detail: None,
                divisor: None,
            }],
        });
        assert!(validate(&def).is_ok());
    }

    #[test]
    fn test_validate_json_remaining_and_limit_conflict() {
        let mut def = make_minimal_def();
        step_mut(&mut def).parser = Some(ParserDef::Json {
            account_email: None,
            account_tier: None,
            quotas: vec![JsonQuotaRule {
                label: "Bad".to_string(),
                used: None,
                limit: Some("data.limit".to_string()),
                remaining: Some("data.remaining".to_string()),
                quota_type: QuotaTypeDef::Credit,
                detail: None,
                divisor: None,
            }],
        });
        let err = validate(&def).unwrap_err();
        assert!(err.to_string().contains("mutually exclusive"));
    }

    #[test]
    fn test_validate_json_limit_without_used() {
        let mut def = make_minimal_def();
        step_mut(&mut def).parser = Some(ParserDef::Json {
            account_email: None,
            account_tier: None,
            quotas: vec![JsonQuotaRule {
                label: "Bad".to_string(),
                used: None,
                limit: Some("data.limit".to_string()),
                remaining: None,
                quota_type: QuotaTypeDef::General,
                detail: None,
                divisor: None,
            }],
        });
        let err = validate(&def).unwrap_err();
        assert!(err.to_string().contains("'used' is required"));
    }

    #[test]
    fn test_validate_json_divisor_zero() {
        let mut def = make_minimal_def();
        step_mut(&mut def).parser = Some(ParserDef::Json {
            account_email: None,
            account_tier: None,
            quotas: vec![JsonQuotaRule {
                label: "Balance".to_string(),
                used: Some("data.used".to_string()),
                limit: Some("data.limit".to_string()),
                remaining: None,
                quota_type: QuotaTypeDef::Credit,
                detail: None,
                divisor: Some(0.0),
            }],
        });
        let err = validate(&def).unwrap_err();
        assert!(err.to_string().contains("divisor must be positive"));
    }

    #[test]
    fn test_validate_regex_divisor_zero() {
        let mut def = make_minimal_def();
        step_mut(&mut def).parser = Some(ParserDef::Regex {
            account_email: None,
            quotas: vec![RegexQuotaRule {
                label: "Credits".to_string(),
                pattern: r"(\d+)/(\d+)".to_string(),
                used_group: 1,
                limit_group: 2,
                quota_type: QuotaTypeDef::General,
                divisor: Some(0.0),
            }],
        });
        let err = validate(&def).unwrap_err();
        assert!(err.to_string().contains("divisor must be positive"));
    }

    #[test]
    fn test_validate_divisor_negative() {
        let mut def = make_minimal_def();
        step_mut(&mut def).parser = Some(ParserDef::Json {
            account_email: None,
            account_tier: None,
            quotas: vec![JsonQuotaRule {
                label: "Balance".to_string(),
                used: Some("data.used".to_string()),
                limit: Some("data.limit".to_string()),
                remaining: None,
                quota_type: QuotaTypeDef::Credit,
                detail: None,
                divisor: Some(-100.0),
            }],
        });
        let err = validate(&def).unwrap_err();
        assert!(err.to_string().contains("divisor must be positive"));
    }

    #[test]
    fn test_validate_divisor_positive_is_ok() {
        let mut def = make_minimal_def();
        step_mut(&mut def).parser = Some(ParserDef::Json {
            account_email: None,
            account_tier: None,
            quotas: vec![JsonQuotaRule {
                label: "Balance".to_string(),
                used: Some("data.used".to_string()),
                limit: Some("data.limit".to_string()),
                remaining: None,
                quota_type: QuotaTypeDef::Credit,
                detail: None,
                divisor: Some(500000.0),
            }],
        });
        assert!(validate(&def).is_ok());
    }

    // ── Phase 3: placeholder validation ──────────

    #[test]
    fn test_validate_placeholder_source_valid() {
        let mut def = make_minimal_def();
        step_mut(&mut def).source = SourceDef::Placeholder {
            reason: "No API available".to_string(),
        };
        step_mut(&mut def).parser = None;
        assert!(validate(&def).is_ok());
    }

    #[test]
    fn test_validate_non_placeholder_requires_parser() {
        let mut def = make_minimal_def();
        step_mut(&mut def).parser = None;

        let err = validate(&def).unwrap_err();
        assert!(err.to_string().contains("parser"));
    }

    #[test]
    fn test_validate_placeholder_source_empty_reason() {
        let mut def = make_minimal_def();
        step_mut(&mut def).source = SourceDef::Placeholder {
            reason: String::new(),
        };
        step_mut(&mut def).parser = None;
        let err = validate(&def).unwrap_err();
        assert!(err.to_string().contains("reason"));
    }

    #[test]
    fn test_load_placeholder_yaml() {
        let dir = tempfile::tempdir().unwrap();
        let yaml = r#"
schema_version: 2
id: "placeholder:test"
metadata:
  display_name: "Placeholder Test"
  brand_name: "Test"
plan:
  steps:
    - name: detect
      availability:
        type: cli_exists
        value: "echo"
      source:
        type: placeholder
        reason: "No public API"
"#;
        fs::write(dir.path().join("placeholder.yaml"), yaml).unwrap();
        let providers = load_from_dir(dir.path());
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].id(), "placeholder:test");
    }

    // ── load_from_dir ───────────────────────────

    #[test]
    fn test_load_from_nonexistent_dir() {
        let providers = load_from_dir(Path::new("/nonexistent/dir/12345"));
        assert!(providers.is_empty());
    }

    #[test]
    fn test_load_from_dir_with_valid_yaml() {
        let dir = tempfile::tempdir().unwrap();
        let yaml = r#"
schema_version: 2
id: "test:cli"
metadata:
  display_name: "Test"
  brand_name: "Test"
plan:
  steps:
    - name: cli
      availability:
        type: cli_exists
        value: "echo"
      source:
        type: cli
        command: "echo"
        args: ["10/100"]
      parser:
        format: regex
        quotas:
          - label: "Usage"
            pattern: '(\d+)/(\d+)'
"#;
        fs::write(dir.path().join("test.yaml"), yaml).unwrap();
        let providers = load_from_dir(dir.path());
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].id(), "test:cli");
    }

    #[test]
    fn test_load_from_dir_skips_invalid_yaml() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("bad.yaml"), "invalid: [yaml").unwrap();
        let providers = load_from_dir(dir.path());
        assert!(providers.is_empty());
    }

    #[test]
    fn test_legacy_yaml_load_error_hints_migration_script() {
        let dir = tempfile::tempdir().unwrap();
        let yaml = r#"
id: "legacy:cli"
metadata:
  display_name: "Legacy"
  brand_name: "Legacy"
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
        let path = dir.path().join("legacy.yaml");
        fs::write(&path, yaml).unwrap();

        // load_one 应在 deserialize 失败后追加迁移脚本提示
        let err = match load_one(&path) {
            Ok(_) => panic!("expected legacy YAML to fail loading"),
            Err(e) => e,
        };
        let msg = err.to_string();
        assert!(
            msg.contains("migrate_custom_provider_yaml.py"),
            "got: {msg}"
        );
        assert!(msg.contains("legacy schema"), "got: {msg}");
    }

    #[test]
    fn test_unknown_top_level_field_rejected() {
        // 拼错字段名（plan -> plain）应被 deny_unknown_fields 拦住
        let yaml = r#"
schema_version: 2
id: "typo:cli"
metadata:
  display_name: "Typo"
  brand_name: "Typo"
plain:
  steps:
    - name: cli
      source:
        type: cli
        command: "echo"
      parser:
        format: regex
        quotas:
          - label: "Usage"
            pattern: '(\d+)/(\d+)'
"#;
        let err = serde_norway::from_str::<CustomProviderDef>(yaml).unwrap_err();
        assert!(err.to_string().contains("unknown field"));
        assert!(err.to_string().contains("plain"));
    }

    #[test]
    fn test_load_from_dir_skips_non_yaml() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("readme.txt"), "not yaml").unwrap();
        let providers = load_from_dir(dir.path());
        assert!(providers.is_empty());
    }

    #[test]
    fn test_load_from_dir_deterministic_order() {
        let dir = tempfile::tempdir().unwrap();
        let yaml_template = |id: &str| {
            format!(
                r#"
schema_version: 2
id: "{id}"
metadata:
  display_name: "{id}"
  brand_name: "Test"
plan:
  steps:
    - name: cli
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
"#
            )
        };
        fs::write(dir.path().join("z-provider.yaml"), yaml_template("z:cli")).unwrap();
        fs::write(dir.path().join("a-provider.yaml"), yaml_template("a:cli")).unwrap();

        let providers = load_from_dir(dir.path());
        assert_eq!(providers.len(), 2);
        assert_eq!(providers[0].id(), "a:cli");
        assert_eq!(providers[1].id(), "z:cli");
    }

    #[test]
    fn test_load_from_dir_validation_rejects_bad_regex() {
        let dir = tempfile::tempdir().unwrap();
        let yaml = r#"
schema_version: 2
id: "bad:cli"
metadata:
  display_name: "Bad"
  brand_name: "Test"
plan:
  steps:
    - name: cli
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
            pattern: '[invalid'
"#;
        fs::write(dir.path().join("bad.yaml"), yaml).unwrap();
        let providers = load_from_dir(dir.path());
        assert!(providers.is_empty());
    }

    #[test]
    fn test_docs_examples_load() {
        let examples_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/examples");
        let providers = load_from_dir(&examples_dir);
        assert_eq!(providers.len(), 6);
    }
}
