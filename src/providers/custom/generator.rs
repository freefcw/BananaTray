//! Custom provider YAML text generator
//!
//! 根据用户输入的必要信息（站点 URL、Session Token 等），
//! 自动生成完整的自定义 Provider YAML 配置文件。
//!
//! 纯数据类型（`NewApiConfig`、`NewApiEditData`）和 ID 计算函数
//! 已迁移至 `models/newapi.rs`，本模块仅保留 YAML 文本生成和纯解析辅助。

use crate::models::newapi::{newapi_provider_id, NewApiConfig, NewApiEditData};
use crate::models::{format_divisor_value, ScriptProviderConfig};

/// 转义 YAML 双引号字符串中的特殊字符
///
/// YAML 双引号字符串中需要转义的关键字符：
/// - `\` → `\\`（反斜杠）
/// - `"` → `\"`（双引号）
fn escape_yaml_double_quoted(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// 根据输入生成完整的 NewAPI YAML 配置
///
/// `id_override` 用于编辑保存时保持 Provider 身份不变（原始 YAML 的 `id`）；
/// 新增时传 `None`，按 base_url + user_id 计算身份（同站多账号见
/// `models::newapi::newapi_provider_id`）。
pub(crate) fn generate_newapi_yaml(config: &NewApiConfig, id_override: Option<&str>) -> String {
    let id = id_override
        .map(str::to_owned)
        .unwrap_or_else(|| newapi_provider_id(&config.base_url, config.user_id.as_deref()));
    let base_url = config.base_url.trim_end_matches('/');
    let divisor = config.divisor.unwrap_or(500_000.0);
    let divisor_text = format_divisor_value(divisor);

    // 转义用户输入，防止 YAML 注入
    let display_name_escaped = escape_yaml_double_quoted(&config.display_name);
    let base_url_escaped = escape_yaml_double_quoted(base_url);
    let cookie_escaped = escape_yaml_double_quoted(&config.cookie);

    // 构建 headers — user_id 仅用于 New-Api-User header，URL 始终使用 /api/user/self
    let headers_block = if let Some(ref uid) = config.user_id {
        let uid = uid.trim();
        if uid.is_empty() {
            String::new()
        } else {
            let escaped_uid = escape_yaml_double_quoted(uid);
            format!(
                "\n        headers:\n          - name: \"New-Api-User\"\n            value: \"{}\"",
                escaped_uid
            )
        }
    } else {
        String::new()
    };

    format!(
        r#"# 自动生成的 NewAPI 中转站配置
# 由 BananaTray 快速添加向导创建

id: "{id}"
schema_version: 2

base_url: "{base_url}"

metadata:
  display_name: "{display_name}"
  brand_name: "NewAPI Relay"
  dashboard_url: "/"
  account_hint: "NewAPI account"
  source_label: "newapi api"

plan:
  mode: first_success
  steps:
    - name: "api"
      required: true
      availability:
        type: always
      source:
        type: http
        method: get
        url: "/api/user/self"
        auth:
          type: cookie
          value: "{cookie}"{headers}
      parser:
        format: json
        account_email: "data.display_name"
        quotas:
          - label: "Balance"
            remaining: "data.quota"
            used: "data.used_quota"
            quota_type: credit
            divisor: {divisor}
"#,
        id = id,
        base_url = base_url_escaped,
        display_name = display_name_escaped,
        cookie = cookie_escaped,
        headers = headers_block,
        divisor = divisor_text,
    )
}

pub(crate) fn generate_script_provider_yaml(
    config: &ScriptProviderConfig,
    script_path: &std::path::Path,
) -> String {
    let id = escape_yaml_double_quoted(&config.provider_id);
    let display_name = escape_yaml_double_quoted(&config.display_name);
    let interpreter = escape_yaml_double_quoted(&config.interpreter);
    let script_path = escape_yaml_double_quoted(&script_path.to_string_lossy());
    let source_label = escape_yaml_double_quoted("script");
    let detail_unit_path = escape_yaml_double_quoted("unit");

    format!(
        r#"# 自动生成的脚本 Provider 配置
# 由 BananaTray 脚本向导创建

id: "{id}"
schema_version: 2

metadata:
  display_name: "{display_name}"
  brand_name: "Custom Script"
  dashboard_url: ""
  account_hint: "script output"
  source_label: "{source_label}"

plan:
  mode: first_success
  steps:
    - name: "script"
      required: true
      availability:
        type: cli_exists
        value: "{interpreter}"
      source:
        type: cli
        command: "{interpreter}"
        timeout_ms: {timeout_ms}
        args:
          - "{script_path}"
      parser:
        format: json
        account_email: "account_email"
        account_tier: "account_tier"
        quotas:
          - label: "Balance"
            remaining: "remaining"
            used: "used"
            quota_type: credit
            detail: "{detail_unit_path}"
"#,
        id = id,
        display_name = display_name,
        source_label = source_label,
        interpreter = interpreter,
        timeout_ms = config.timeout_ms,
        script_path = script_path,
        detail_unit_path = detail_unit_path,
    )
}

/// 从已解析的 CustomProviderDef 中提取 NewApiEditData（纯函数，无 I/O）。
///
/// 由 `providers::custom::api` 组合磁盘 I/O 后调用。
pub(in crate::providers::custom) fn parse_newapi_edit_data(
    def: &super::schema::CustomProviderDef,
    original_filename: String,
) -> Option<NewApiEditData> {
    use super::schema::{AuthDef, SourceDef};

    // 从第一个 step 的 SourceDef 提取 cookie 和 headers（非 HTTP 的返回 None）
    let step = def.plan.steps.first()?;
    let (cookie, user_id) = match &step.source {
        SourceDef::Http { auth, headers, .. } => {
            let cookie = match auth {
                Some(AuthDef::Cookie { value }) => value.clone(),
                Some(AuthDef::SessionToken { token, cookie_name }) => {
                    format!("{}={}", cookie_name, token)
                }
                _ => String::new(),
            };
            // 从 headers 中查找 New-Api-User
            let uid = headers
                .iter()
                .find(|h| h.name == "New-Api-User")
                .map(|h| h.value.clone());
            (cookie, uid)
        }
        _ => return None, // 非 HTTP GET 的不支持编辑
    };

    // 从 parser 提取 divisor
    let divisor = step.parser.as_ref().and_then(|p| {
        if let super::schema::ParserDef::Json { quotas, .. } = p {
            quotas.first().and_then(|q| q.divisor)
        } else {
            None
        }
    });

    Some(NewApiEditData {
        display_name: def.metadata.display_name.clone(),
        base_url: def.base_url.clone().unwrap_or_default(),
        cookie,
        user_id,
        divisor,
        original_filename,
        original_id: def.id.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::newapi::extract_domain_slug;

    fn make_config() -> NewApiConfig {
        NewApiConfig {
            display_name: "Test API".to_string(),
            base_url: "https://my-api.example.com".to_string(),
            cookie: "session=eyJhbGciOiJIUzI1NiJ9".to_string(),
            user_id: None,
            divisor: None,
        }
    }

    #[test]
    fn test_extract_domain_slug() {
        assert_eq!(
            extract_domain_slug("https://my-api.example.com"),
            "my-api-example-com"
        );
        assert_eq!(
            extract_domain_slug("http://localhost:3000"),
            "localhost-3000"
        );
        assert_eq!(extract_domain_slug("https://api.site.io/"), "api-site-io");
    }

    #[test]
    fn test_generate_yaml_contains_essential_fields() {
        let config = make_config();
        let yaml = generate_newapi_yaml(&config, None);

        assert!(yaml.contains("id: \"my-api-example-com:newapi\""));
        assert!(yaml.contains("base_url: \"https://my-api.example.com\""));
        assert!(yaml.contains("display_name: \"Test API\""));
        assert!(yaml.contains("type: cookie"));
        assert!(yaml.contains("value: \"session=eyJhbGciOiJIUzI1NiJ9\""));
        assert!(yaml.contains("url: \"/api/user/self\""));
        assert!(yaml.contains("divisor: 500000"));
    }

    #[test]
    fn test_generate_yaml_with_full_cookie_string() {
        // 用户直接粘贴完整 Cookie header
        let config = NewApiConfig {
            cookie: "session=eyJ123; cf_clearance=abc456; _ga=xxx".to_string(),
            ..make_config()
        };
        let yaml = generate_newapi_yaml(&config, None);

        assert!(yaml.contains("type: cookie"));
        assert!(yaml.contains("session=eyJ123; cf_clearance=abc456; _ga=xxx"));
    }

    #[test]
    fn test_generate_yaml_with_user_id() {
        let config = NewApiConfig {
            user_id: Some("12345".to_string()),
            ..make_config()
        };
        let yaml = generate_newapi_yaml(&config, None);

        // URL 始终为 /api/user/self
        assert!(yaml.contains("url: \"/api/user/self\""));
        // user_id 仅用于 New-Api-User header
        assert!(yaml.contains("New-Api-User"));
        assert!(yaml.contains("value: \"12345\""));
    }

    #[test]
    fn test_generate_yaml_with_empty_user_id_falls_back_to_self() {
        let config = NewApiConfig {
            user_id: Some("  ".to_string()),
            ..make_config()
        };
        let yaml = generate_newapi_yaml(&config, None);

        assert!(yaml.contains("url: \"/api/user/self\""));
    }

    #[test]
    fn test_generate_yaml_with_custom_divisor() {
        let config = NewApiConfig {
            divisor: Some(1_000_000.0),
            ..make_config()
        };
        let yaml = generate_newapi_yaml(&config, None);

        assert!(yaml.contains("divisor: 1000000"));
    }

    #[test]
    fn test_generate_yaml_with_fractional_divisor() {
        let config = NewApiConfig {
            divisor: Some(0.5),
            ..make_config()
        };
        let yaml = generate_newapi_yaml(&config, None);

        assert!(yaml.contains("divisor: 0.5"));
    }

    #[test]
    fn test_generate_yaml_trailing_slash_stripped() {
        let config = NewApiConfig {
            base_url: "https://my-api.example.com/".to_string(),
            ..make_config()
        };
        let yaml = generate_newapi_yaml(&config, None);

        assert!(yaml.contains("base_url: \"https://my-api.example.com\""));
    }

    #[test]
    fn test_generate_yaml_is_valid_custom_provider_def() {
        let config = make_config();
        let yaml = generate_newapi_yaml(&config, None);

        let def: crate::providers::custom::schema::CustomProviderDef =
            serde_norway::from_str(&yaml).expect("Generated YAML should be valid");

        assert_eq!(def.id, "my-api-example-com:newapi");
        assert_eq!(def.metadata.display_name, "Test API");
        assert_eq!(def.base_url.as_deref(), Some("https://my-api.example.com"));

        assert_eq!(def.schema_version, 2);
        let step = def.plan.steps.first().expect("should have one step");
        // 验证使用 cookie auth 类型
        if let crate::providers::custom::schema::SourceDef::Http { auth, .. } = &step.source {
            assert!(matches!(
                auth.as_ref().unwrap(),
                crate::providers::custom::schema::AuthDef::Cookie { .. }
            ));
        } else {
            panic!("Expected HTTP source");
        }
    }

    #[test]
    fn test_generate_yaml_with_user_id_is_valid_and_has_header() {
        let config = NewApiConfig {
            user_id: Some("42".to_string()),
            ..make_config()
        };
        let yaml = generate_newapi_yaml(&config, None);

        let def: crate::providers::custom::schema::CustomProviderDef =
            serde_norway::from_str(&yaml).expect("Generated YAML with user_id should be valid");

        // user_id 参与身份：同站多账号通过 ID 区分
        assert_eq!(def.id, "my-api-example-com-42:newapi");

        // URL 始终为 /api/user/self，user_id 仅用于 header
        let step = def.plan.steps.first().expect("should have one step");
        if let crate::providers::custom::schema::SourceDef::Http { url, headers, .. } = &step.source
        {
            assert_eq!(url, "/api/user/self");
            // 验证 New-Api-User header 存在
            assert_eq!(headers.len(), 1);
            assert_eq!(headers[0].name, "New-Api-User");
            assert_eq!(headers[0].value, "42");
        } else {
            panic!("Expected HTTP source");
        }
    }

    #[test]
    fn test_generate_yaml_id_override_preserves_identity() {
        // 编辑保存：user_id 变化不改变身份，YAML 写回原始 ID
        let config = NewApiConfig {
            user_id: Some("42".to_string()),
            ..make_config()
        };
        let yaml = generate_newapi_yaml(&config, Some("my-api-example-com:newapi"));

        assert!(yaml.contains("id: \"my-api-example-com:newapi\""));
    }

    #[test]
    fn test_escape_yaml_double_quoted() {
        assert_eq!(
            escape_yaml_double_quoted(r#"hello"world"#),
            r#"hello\"world"#
        );
        assert_eq!(escape_yaml_double_quoted(r"path\to"), r"path\\to");
        assert_eq!(escape_yaml_double_quoted(r#"a"b\c"#), r#"a\"b\\c"#);
        assert_eq!(escape_yaml_double_quoted("normal"), "normal");
    }

    #[test]
    fn test_generate_yaml_with_special_chars_is_valid() {
        let config = NewApiConfig {
            display_name: r#"My "API" Site"#.to_string(),
            cookie: r#"session=tok"with\special"#.to_string(),
            ..make_config()
        };
        let yaml = generate_newapi_yaml(&config, None);

        assert!(yaml.contains(r#"display_name: "My \"API\" Site""#));

        let def: crate::providers::custom::schema::CustomProviderDef =
            serde_norway::from_str(&yaml).expect("YAML with special chars should be valid");
        assert_eq!(def.metadata.display_name, r#"My "API" Site"#);
    }

    #[test]
    fn test_generate_script_provider_yaml_is_valid() {
        let config = ScriptProviderConfig {
            display_name: r#"My "Script""#.to_string(),
            provider_id: "script:custom".to_string(),
            interpreter: "python3".to_string(),
            timeout_ms: 12_000,
            script: "print('{}')".to_string(),
        };
        let script_path = std::path::Path::new("/tmp/banana script.py");
        let yaml = generate_script_provider_yaml(&config, script_path);

        let def: crate::providers::custom::schema::CustomProviderDef =
            serde_norway::from_str(&yaml).expect("Generated script YAML should be valid");
        assert_eq!(def.id, "script:custom");
        assert_eq!(def.metadata.display_name, r#"My "Script""#);

        let step = def.plan.steps.first().expect("should have one step");
        match &step.source {
            crate::providers::custom::schema::SourceDef::Cli {
                command,
                args,
                timeout_ms,
                ..
            } => {
                assert_eq!(command, "python3");
                assert_eq!(args, &[script_path.to_string_lossy().to_string()]);
                assert_eq!(*timeout_ms, Some(12_000));
            }
            other => panic!("Expected CLI source, got {other:?}"),
        }
    }

    // ── roundtrip: generate → parse ──────────────────────────

    /// 辅助：生成 YAML → 解析为 CustomProviderDef → 提取 NewApiEditData
    fn roundtrip(config: &NewApiConfig) -> NewApiEditData {
        let yaml = generate_newapi_yaml(config, None);
        let filename = format!("newapi-{}.yaml", extract_domain_slug(&config.base_url));
        let def: crate::providers::custom::schema::CustomProviderDef =
            serde_norway::from_str(&yaml).expect("Generated YAML must be parseable");
        parse_newapi_edit_data(&def, filename).expect("parse_newapi_edit_data must succeed")
    }

    #[test]
    fn roundtrip_basic_config() {
        let config = make_config();
        let edit = roundtrip(&config);

        assert_eq!(edit.display_name, "Test API");
        assert_eq!(edit.base_url, "https://my-api.example.com");
        assert_eq!(edit.cookie, "session=eyJhbGciOiJIUzI1NiJ9");
        assert!(edit.user_id.is_none());
        // 默认 divisor 是 500000
        assert_eq!(edit.divisor, Some(500000.0));
        assert_eq!(edit.original_filename, "newapi-my-api-example-com.yaml");
    }

    #[test]
    fn roundtrip_with_user_id() {
        let config = NewApiConfig {
            user_id: Some("42".to_string()),
            ..make_config()
        };
        let edit = roundtrip(&config);

        assert_eq!(edit.user_id.as_deref(), Some("42"));
        assert_eq!(edit.cookie, "session=eyJhbGciOiJIUzI1NiJ9");
    }

    #[test]
    fn roundtrip_with_custom_divisor() {
        let config = NewApiConfig {
            divisor: Some(1_000_000.0),
            ..make_config()
        };
        let edit = roundtrip(&config);

        assert_eq!(edit.divisor, Some(1_000_000.0));
    }

    #[test]
    fn roundtrip_preserves_fractional_divisor() {
        let config = NewApiConfig {
            divisor: Some(0.5),
            ..make_config()
        };
        let edit = roundtrip(&config);

        assert_eq!(edit.divisor, Some(0.5));
    }

    #[test]
    fn roundtrip_preserves_full_cookie() {
        let config = NewApiConfig {
            cookie: "session=eyJ123; cf_clearance=abc456; _ga=xxx".to_string(),
            ..make_config()
        };
        let edit = roundtrip(&config);

        assert_eq!(edit.cookie, "session=eyJ123; cf_clearance=abc456; _ga=xxx");
    }
}
