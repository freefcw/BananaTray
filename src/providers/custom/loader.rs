use anyhow::Result;
use log::{info, warn};
use regex::Regex;
use std::path::{Path, PathBuf};

use super::provider::CustomProvider;
use super::schema::{CustomProviderDef, ParserDef, RegexQuotaRule, SourceDef};

/// 自定义 Provider YAML 文件的搜索目录
fn providers_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("bananatray")
        .join("providers")
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
    let def: CustomProviderDef = serde_yaml::from_str(&content)?;
    validate(&def)?;
    Ok(CustomProvider::new(def))
}

/// 校验定义的合法性，在加载时 fail-fast
fn validate(def: &CustomProviderDef) -> Result<()> {
    if def.id.is_empty() {
        anyhow::bail!("'id' cannot be empty");
    }
    if def.metadata.display_name.is_empty() {
        anyhow::bail!("'metadata.display_name' cannot be empty");
    }

    validate_source(&def.source)?;
    validate_parser(&def.parser)?;

    Ok(())
}

fn validate_source(source: &SourceDef) -> Result<()> {
    match source {
        SourceDef::Cli { command, .. } => {
            if command.is_empty() {
                anyhow::bail!("'source.command' cannot be empty");
            }
        }
        SourceDef::HttpGet { url, .. } | SourceDef::HttpPost { url, .. } => {
            if url.is_empty() {
                anyhow::bail!("'source.url' cannot be empty");
            }
        }
    }
    Ok(())
}

fn validate_parser(parser: &ParserDef) -> Result<()> {
    match parser {
        ParserDef::Json { quotas, .. } => {
            if quotas.is_empty() {
                anyhow::bail!("'parser.quotas' must contain at least one rule");
            }
            for rule in quotas {
                if rule.used.is_empty() || rule.limit.is_empty() {
                    anyhow::bail!(
                        "quota rule '{}': 'used' and 'limit' paths cannot be empty",
                        rule.label
                    );
                }
            }
        }
        ParserDef::Regex { quotas, .. } => {
            if quotas.is_empty() {
                anyhow::bail!("'parser.quotas' must contain at least one rule");
            }
            for rule in quotas {
                validate_regex_rule(rule)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::custom::schema::*;
    use std::fs;

    fn make_minimal_def() -> CustomProviderDef {
        CustomProviderDef {
            id: "test:cli".to_string(),
            metadata: MetadataDef {
                display_name: "Test".to_string(),
                brand_name: "Test".to_string(),
                icon: "🤖".to_string(),
                dashboard_url: String::new(),
                account_hint: "account".to_string(),
                source_label: String::new(),
            },
            availability: AvailabilityDef::CliExists {
                value: "echo".to_string(),
            },
            source: SourceDef::Cli {
                command: "echo".to_string(),
                args: vec![],
            },
            parser: ParserDef::Regex {
                account_email: None,
                quotas: vec![RegexQuotaRule {
                    label: "Usage".to_string(),
                    pattern: r"(\d+)/(\d+)".to_string(),
                    used_group: 1,
                    limit_group: 2,
                    quota_type: QuotaTypeDef::General,
                }],
            },
        }
    }

    // ── validate ────────────────────────────────

    #[test]
    fn test_validate_valid() {
        assert!(validate(&make_minimal_def()).is_ok());
    }

    #[test]
    fn test_validate_empty_id() {
        let mut def = make_minimal_def();
        def.id = String::new();
        assert!(validate(&def).is_err());
    }

    #[test]
    fn test_validate_empty_display_name() {
        let mut def = make_minimal_def();
        def.metadata.display_name = String::new();
        assert!(validate(&def).is_err());
    }

    #[test]
    fn test_validate_empty_command() {
        let mut def = make_minimal_def();
        def.source = SourceDef::Cli {
            command: String::new(),
            args: vec![],
        };
        assert!(validate(&def).is_err());
    }

    #[test]
    fn test_validate_empty_url() {
        let mut def = make_minimal_def();
        def.source = SourceDef::HttpGet {
            url: String::new(),
            auth: None,
            headers: vec![],
        };
        assert!(validate(&def).is_err());
    }

    #[test]
    fn test_validate_empty_quotas() {
        let mut def = make_minimal_def();
        def.parser = ParserDef::Regex {
            account_email: None,
            quotas: vec![],
        };
        assert!(validate(&def).is_err());
    }

    #[test]
    fn test_validate_invalid_regex() {
        let mut def = make_minimal_def();
        def.parser = ParserDef::Regex {
            account_email: None,
            quotas: vec![RegexQuotaRule {
                label: "Bad".to_string(),
                pattern: "[invalid".to_string(),
                used_group: 1,
                limit_group: 2,
                quota_type: QuotaTypeDef::General,
            }],
        };
        let err = validate(&def).unwrap_err();
        assert!(err.to_string().contains("invalid regex"));
    }

    #[test]
    fn test_validate_bad_capture_group() {
        let mut def = make_minimal_def();
        def.parser = ParserDef::Regex {
            account_email: None,
            quotas: vec![RegexQuotaRule {
                label: "Bad".to_string(),
                pattern: r"(\d+)".to_string(), // 只有 1 个 group
                used_group: 1,
                limit_group: 5, // 超出
                quota_type: QuotaTypeDef::General,
            }],
        };
        let err = validate(&def).unwrap_err();
        assert!(err.to_string().contains("limit_group 5"));
    }

    #[test]
    fn test_validate_empty_json_paths() {
        let mut def = make_minimal_def();
        def.parser = ParserDef::Json {
            account_email: None,
            account_tier: None,
            quotas: vec![JsonQuotaRule {
                label: "Test".to_string(),
                used: String::new(),
                limit: "usage.limit".to_string(),
                quota_type: QuotaTypeDef::General,
                detail: None,
            }],
        };
        assert!(validate(&def).is_err());
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
id: "test:cli"
metadata:
  display_name: "Test"
  brand_name: "Test"
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
id: "{id}"
metadata:
  display_name: "{id}"
  brand_name: "Test"
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
id: "bad:cli"
metadata:
  display_name: "Bad"
  brand_name: "Test"
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
}
