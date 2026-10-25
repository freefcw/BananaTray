use anyhow::Result;
use log::{info, warn};

use std::path::{Path, PathBuf};

use super::legacy_migrate::migrate_text;
use super::provider::CustomProvider;
use super::schema::CustomProviderDef;
use super::validate::validate;

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
    let def = read_provider_def(path)?;
    validate(&def)?;
    CustomProvider::new(def)
}

/// 读取自定义 Provider YAML；旧 schema 会先自动迁移并尽量写回磁盘。
///
/// 仅限启动加载路径（`load_from_dir`）使用——迁移写回是加载时的显式行为；
/// 定位/编辑回填等纯读取场景必须走 [`parse_provider_def`]，不得触碰磁盘。
pub(super) fn read_provider_def(path: &Path) -> Result<CustomProviderDef> {
    let original = std::fs::read_to_string(path)?;
    let (def, migrated) = parse_with_optional_migration(&original)?;
    if let Some(migrated) = migrated.as_deref() {
        match persist_migrated_yaml(path, migrated) {
            Ok(backup) => info!(
                target: "providers::custom",
                "Migrated legacy custom provider YAML {} → schema_version 2 (backup: {})",
                path.display(),
                backup.display()
            ),
            Err(err) => warn!(
                target: "providers::custom",
                "Migrated {} in memory but failed to write schema_version 2 YAML: {}",
                path.display(),
                err
            ),
        }
    }
    Ok(def)
}

/// 解析自定义 Provider YAML 内容（含旧 schema 的内存迁移），不产生任何磁盘写入。
///
/// 供 locator 等"查找/定位"语义的调用方使用：旧 schema 只在返回值中升到 v2，
/// 磁盘上的迁移写回发生在下一次启动加载（`load_from_dir`）时。
/// 门控与 locator 一致：纯 lib（--no-default-features）构建没有定位场景。
#[cfg(any(feature = "app", test))]
pub(super) fn parse_provider_def(content: &str) -> Result<CustomProviderDef> {
    Ok(parse_with_optional_migration(content)?.0)
}

fn parse_with_optional_migration(content: &str) -> Result<(CustomProviderDef, Option<String>)> {
    match serde_norway::from_str::<CustomProviderDef>(content) {
        Ok(def) => Ok((def, None)),
        Err(err) => match migrate_text(content) {
            Ok((migrated, true)) => {
                let def = serde_norway::from_str(&migrated).map_err(|migrate_err| {
                    anyhow::anyhow!(
                        "{err}; auto-migration produced invalid schema_version 2 YAML: {migrate_err}"
                    )
                })?;
                Ok((def, Some(migrated)))
            }
            Ok((_, false)) => Err(augment_legacy_schema_hint(err, content)),
            Err(migrate_err) => Err(augment_legacy_schema_hint_with_cause(
                err,
                content,
                &migrate_err.to_string(),
            )),
        },
    }
}

/// 当 deserialize 失败且 YAML 看起来像旧 schema 时，补上自动迁移失败原因。
///
/// `deny_unknown_fields` 让旧 YAML 顶层 `availability/source/parser` 在 deserialize
/// 阶段就死于 `unknown field`，会丢掉 validate 阶段对 `schema_version` 的友好提示。
fn augment_legacy_schema_hint(err: serde_norway::Error, content: &str) -> anyhow::Error {
    if looks_like_legacy_schema(content) {
        anyhow::anyhow!(
            "{err}; YAML appears to use the legacy schema (top-level source/parser), \
             but auto-migration did not change the file"
        )
    } else {
        anyhow::Error::from(err)
    }
}

fn augment_legacy_schema_hint_with_cause(
    err: serde_norway::Error,
    content: &str,
    cause: &str,
) -> anyhow::Error {
    if looks_like_legacy_schema(content) {
        anyhow::anyhow!(
            "{err}; YAML appears to use the legacy schema (top-level source/parser), \
             but auto-migration failed: {cause}"
        )
    } else {
        anyhow::Error::from(err)
    }
}

fn looks_like_legacy_schema(content: &str) -> bool {
    content.lines().any(|line| {
        line.starts_with("source:")
            || line.starts_with("availability:")
            || line.starts_with("parser:")
    })
}

fn persist_migrated_yaml(path: &Path, migrated: &str) -> std::io::Result<PathBuf> {
    let backup = unique_backup_path(path);
    std::fs::copy(path, &backup)?;
    // 继承原文件权限（用户可能自行收紧过）；但临时文件先以 0600 独占创建，
    // 避免含 token 的迁移内容以默认权限（常见 0644）短暂对外可读。
    let permissions = path.metadata()?.permissions();
    let tmp = crate::platform::atomic_file::temp_sibling_path(path);
    let write_result = (|| {
        crate::platform::atomic_file::write_private_file_exclusively(&tmp, migrated.as_bytes())?;
        std::fs::set_permissions(&tmp, permissions)?;
        std::fs::rename(&tmp, path)
    })();
    if write_result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    write_result.map(|()| backup)
}

fn unique_backup_path(path: &Path) -> PathBuf {
    let backup = {
        let mut backup = path.as_os_str().to_os_string();
        backup.push(".bak");
        PathBuf::from(backup)
    };
    if !backup.exists() {
        return backup;
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    let mut stamped = path.as_os_str().to_os_string();
    stamped.push(format!(".bak.{stamp}"));
    PathBuf::from(stamped)
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
    fn test_legacy_yaml_is_migrated_written_and_loaded() {
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

        let providers = load_from_dir(dir.path());
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].id(), "legacy:cli");

        let migrated = fs::read_to_string(&path).unwrap();
        assert!(migrated.contains("schema_version: 2"));
        assert!(migrated.contains("plan:"));
        assert!(!migrated.contains("\navailability:"));

        let backup = {
            let mut backup = path.as_os_str().to_os_string();
            backup.push(".bak");
            PathBuf::from(backup)
        };
        assert_eq!(fs::read_to_string(backup).unwrap(), yaml);
    }

    /// 迁移写回必须继承原文件权限：用户把 YAML 收紧到 0600 后，
    /// 落盘的 v2 文件不得回到默认权限（umask 下常见 0644）。
    #[cfg(unix)]
    #[test]
    fn test_migration_write_preserves_owner_only_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let yaml = r#"
id: "legacy-perms:cli"
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
        let path = dir.path().join("legacy-perms.yaml");
        fs::write(&path, yaml).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();

        let providers = load_from_dir(dir.path());
        assert_eq!(providers.len(), 1, "legacy YAML 应迁移并加载成功");

        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "迁移写回不得放宽原文件权限");

        // 目录内只应留下迁移后的 yaml 与 .bak 备份，无 .tmp 残留
        let entries: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(entries.len(), 2, "unexpected files: {entries:?}");
        assert!(entries.iter().all(|name| !name.contains(".tmp")));
    }

    #[test]
    fn test_generated_newapi_legacy_yaml_is_loaded() {
        let dir = tempfile::tempdir().unwrap();
        let yaml = r#"# 自动生成的 NewAPI 中转站配置
id: "anyrouter-top:newapi"
base_url: "https://anyrouter.top"
metadata:
  display_name: "AnyRouter"
  brand_name: "NewAPI Relay"
availability:
  type: always
source:
  type: http_get
  url: "/api/user/self"
  auth:
    type: cookie
    value: "session=test-token"
parser:
  format: json
  account_email: "data.display_name"
  quotas:
    - label: "Balance"
      remaining: "data.quota"
      used: "data.used_quota"
      quota_type: credit
      divisor: 500000
"#;
        fs::write(dir.path().join("newapi-anyrouter-top.yaml"), yaml).unwrap();
        let providers = load_from_dir(dir.path());
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].id(), "anyrouter-top:newapi");
    }

    #[test]
    fn test_unmigratable_legacy_yaml_explains_auto_migration_failure() {
        let dir = tempfile::tempdir().unwrap();
        let yaml = r#"
id: "legacy:cli"
metadata:
  display_name: "Legacy"
  brand_name: "Legacy"
custom_timeout: 42
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

        let err = match load_one(&path) {
            Ok(_) => panic!("expected unmigratable YAML to fail loading"),
            Err(e) => e,
        };
        let msg = err.to_string();
        assert!(msg.contains("legacy schema"), "got: {msg}");
        assert!(msg.contains("auto-migration failed"), "got: {msg}");
        assert!(msg.contains("custom_timeout"), "got: {msg}");
        assert_eq!(fs::read_to_string(&path).unwrap(), yaml);
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
