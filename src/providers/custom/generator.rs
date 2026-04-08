/// NewAPI 中转站 YAML 配置生成器
///
/// 根据用户输入的必要信息（站点 URL、Session Token 等），
/// 自动生成完整的自定义 Provider YAML 配置文件。
///
/// NewAPI 配置输入
#[derive(Debug, Clone)]
pub struct NewApiConfig {
    /// 显示名称，如 "我的 NewAPI 站"
    pub display_name: String,
    /// 站点 URL，如 "https://your-site.com"（不含末尾斜杠）
    pub base_url: String,
    /// 完整的 Cookie 字符串（从浏览器 DevTools 复制）
    /// 如 "session=eyJ...; cf_clearance=abc123"
    pub cookie: String,
    /// 用户 ID（部分站点需要，可选）
    pub user_id: Option<String>,
    /// 积分换算比例（默认 500000 积分 = $1 USD）
    pub divisor: Option<f64>,
}

/// 从 base_url 中提取域名部分，用于生成 id 和文件名
///
/// 例如：
/// - `https://my-api.example.com` → `my-api-example-com`
/// - `http://localhost:3000` → `localhost-3000`
fn extract_domain_slug(base_url: &str) -> String {
    let url = base_url
        .trim_end_matches('/')
        .replace("https://", "")
        .replace("http://", "");

    // 替换非字母数字字符为连字符，去除多余连字符
    url.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// 生成 YAML 配置文件名
pub fn generate_filename(config: &NewApiConfig) -> String {
    let slug = extract_domain_slug(&config.base_url);
    format!("newapi-{}.yaml", slug)
}

/// 转义 YAML 双引号字符串中的特殊字符
///
/// YAML 双引号字符串中需要转义的关键字符：
/// - `\` → `\\`（反斜杠）
/// - `"` → `\"`（双引号）
fn escape_yaml_double_quoted(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// 根据输入生成完整的 NewAPI YAML 配置
pub fn generate_newapi_yaml(config: &NewApiConfig) -> String {
    let slug = extract_domain_slug(&config.base_url);
    let id = format!("{}:newapi", slug);
    let base_url = config.base_url.trim_end_matches('/');
    let divisor = config.divisor.unwrap_or(500_000.0);

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
                "\n  headers:\n    - name: \"New-Api-User\"\n      value: \"{}\"",
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

base_url: "{base_url}"

metadata:
  display_name: "{display_name}"
  brand_name: "NewAPI Relay"
  dashboard_url: "/"
  account_hint: "NewAPI account"
  source_label: "newapi api"

availability:
  type: always

source:
  type: http_get
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
        divisor = divisor as u64,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_generate_filename() {
        let config = make_config();
        assert_eq!(generate_filename(&config), "newapi-my-api-example-com.yaml");
    }

    #[test]
    fn test_generate_yaml_contains_essential_fields() {
        let config = make_config();
        let yaml = generate_newapi_yaml(&config);

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
        let yaml = generate_newapi_yaml(&config);

        assert!(yaml.contains("type: cookie"));
        assert!(yaml.contains("session=eyJ123; cf_clearance=abc456; _ga=xxx"));
    }

    #[test]
    fn test_generate_yaml_with_user_id() {
        let config = NewApiConfig {
            user_id: Some("12345".to_string()),
            ..make_config()
        };
        let yaml = generate_newapi_yaml(&config);

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
        let yaml = generate_newapi_yaml(&config);

        assert!(yaml.contains("url: \"/api/user/self\""));
    }

    #[test]
    fn test_generate_yaml_with_custom_divisor() {
        let config = NewApiConfig {
            divisor: Some(1_000_000.0),
            ..make_config()
        };
        let yaml = generate_newapi_yaml(&config);

        assert!(yaml.contains("divisor: 1000000"));
    }

    #[test]
    fn test_generate_yaml_trailing_slash_stripped() {
        let config = NewApiConfig {
            base_url: "https://my-api.example.com/".to_string(),
            ..make_config()
        };
        let yaml = generate_newapi_yaml(&config);

        assert!(yaml.contains("base_url: \"https://my-api.example.com\""));
    }

    #[test]
    fn test_generate_yaml_is_valid_custom_provider_def() {
        let config = make_config();
        let yaml = generate_newapi_yaml(&config);

        let def: crate::providers::custom::schema::CustomProviderDef =
            serde_yaml::from_str(&yaml).expect("Generated YAML should be valid");

        assert_eq!(def.id, "my-api-example-com:newapi");
        assert_eq!(def.metadata.display_name, "Test API");
        assert_eq!(def.base_url.as_deref(), Some("https://my-api.example.com"));

        // 验证使用 cookie auth 类型
        if let crate::providers::custom::schema::SourceDef::HttpGet { auth, .. } = &def.source {
            assert!(matches!(
                auth.as_ref().unwrap(),
                crate::providers::custom::schema::AuthDef::Cookie { .. }
            ));
        } else {
            panic!("Expected HttpGet source");
        }
    }

    #[test]
    fn test_generate_yaml_with_user_id_is_valid_and_has_header() {
        let config = NewApiConfig {
            user_id: Some("42".to_string()),
            ..make_config()
        };
        let yaml = generate_newapi_yaml(&config);

        let def: crate::providers::custom::schema::CustomProviderDef =
            serde_yaml::from_str(&yaml).expect("Generated YAML with user_id should be valid");

        // URL 始终为 /api/user/self，user_id 仅用于 header
        if let crate::providers::custom::schema::SourceDef::HttpGet { url, headers, .. } =
            &def.source
        {
            assert_eq!(url, "/api/user/self");
            // 验证 New-Api-User header 存在
            assert_eq!(headers.len(), 1);
            assert_eq!(headers[0].name, "New-Api-User");
            assert_eq!(headers[0].value, "42");
        } else {
            panic!("Expected HttpGet source");
        }
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
        let yaml = generate_newapi_yaml(&config);

        assert!(yaml.contains(r#"display_name: "My \"API\" Site""#));

        let def: crate::providers::custom::schema::CustomProviderDef =
            serde_yaml::from_str(&yaml).expect("YAML with special chars should be valid");
        assert_eq!(def.metadata.display_name, r#"My "API" Site"#);
    }
}
