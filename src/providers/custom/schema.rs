use serde::Deserialize;

/// 自定义 Provider 的 YAML 定义（顶层结构）
#[derive(Debug, Clone, Deserialize)]
pub struct CustomProviderDef {
    /// 唯一标识符，如 "myai:cli"
    pub id: String,
    /// 基础 URL（可选），其他 URL 字段若以 / 开头则自动拼接此前缀
    #[serde(default)]
    pub base_url: Option<String>,
    /// 展示元数据
    pub metadata: MetadataDef,
    /// 可用性检查规则
    pub availability: AvailabilityDef,
    /// 数据获取方式
    pub source: SourceDef,
    /// 响应解析规则
    pub parser: ParserDef,
}

/// Provider 展示元数据
#[derive(Debug, Clone, Deserialize)]
pub struct MetadataDef {
    pub display_name: String,
    pub brand_name: String,
    #[serde(default = "default_icon")]
    pub icon: String,
    #[serde(default)]
    pub dashboard_url: String,
    #[serde(default = "default_account_hint")]
    pub account_hint: String,
    #[serde(default)]
    pub source_label: String,
}

fn default_icon() -> String {
    "🤖".to_string()
}

fn default_account_hint() -> String {
    "account".to_string()
}

/// 可用性检查方式
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AvailabilityDef {
    /// 检查 CLI 命令是否存在
    CliExists { value: String },
    /// 检查环境变量是否设置
    EnvVar { value: String },
    /// 检查文件是否存在（支持 ~ 展开）
    FileExists { value: String },
    /// 始终可用（认证信息已在 YAML 中配置，无需前置检查）
    Always,
}

/// 数据获取方式
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SourceDef {
    /// 执行 CLI 命令
    Cli {
        command: String,
        #[serde(default)]
        args: Vec<String>,
    },
    /// HTTP GET 请求
    HttpGet {
        url: String,
        #[serde(default)]
        auth: Option<AuthDef>,
        #[serde(default)]
        headers: Vec<HeaderDef>,
    },
    /// HTTP POST 请求（JSON body）
    HttpPost {
        url: String,
        #[serde(default)]
        auth: Option<AuthDef>,
        #[serde(default)]
        headers: Vec<HeaderDef>,
        #[serde(default)]
        body: String,
    },
}

/// 认证方式
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthDef {
    /// Bearer token 直接写在配置中
    Bearer { token: String },
    /// 从环境变量读取 Bearer token
    BearerEnv { env_var: String },
    /// 从环境变量读取自定义 header 值
    HeaderEnv { header: String, env_var: String },
    /// 通过登录接口获取 access token（备选方案）
    ///
    /// ⚠️ 大部分 NewAPI 站点启用了 Turnstile 等验证，此方式可能无法使用。
    /// 推荐优先使用 `SessionToken` 方式。
    ///
    /// 流程：POST login_url + {"username":"..","password":".."} → 提取 token → Bearer
    Login {
        /// 登录接口 URL（如 https://site.com/api/user/login）
        login_url: String,
        /// 用户名
        username: String,
        /// 密码
        password: String,
        /// 从登录响应中提取 token 的 JSON 路径（默认 "data"）
        #[serde(default = "default_token_path")]
        token_path: String,
    },
    /// 使用原始 Cookie header 值进行认证
    ///
    /// 当需要传递多个 cookie（如 session + cf_clearance）时，
    /// 可使用此方式传递完整的 cookie 字符串
    Cookie {
        /// Cookie 字符串（如 "session=eyJ...;other=val"）
        value: String,
    },
    /// 使用 session token 作为 Cookie 认证（NewAPI/OneAPI 推荐方式）
    ///
    /// 从浏览器 DevTools → Cookies 中复制 session 值即可，无需账号密码。
    /// 自动拼接为 Cookie: <cookie_name>=<token> header
    SessionToken {
        /// session token 值（如 "eyJhbGci..."）
        token: String,
        /// Cookie 中的 key 名称（默认 "session"）
        #[serde(default = "default_session_key")]
        cookie_name: String,
    },
}

fn default_token_path() -> String {
    "data".to_string()
}

fn default_session_key() -> String {
    "session".to_string()
}

/// 自定义 HTTP header
#[derive(Debug, Clone, Deserialize)]
pub struct HeaderDef {
    pub name: String,
    pub value: String,
}

/// 响应解析规则
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "format", rename_all = "snake_case")]
pub enum ParserDef {
    /// JSON 响应解析
    Json {
        /// 账户邮箱的 JSON 路径（可选）
        #[serde(default)]
        account_email: Option<String>,
        /// 账户等级的 JSON 路径（可选）
        #[serde(default)]
        account_tier: Option<String>,
        /// 配额提取规则
        quotas: Vec<JsonQuotaRule>,
    },
    /// 正则表达式解析
    Regex {
        /// 账户邮箱的正则（可选，第一个 capture group）
        #[serde(default)]
        account_email: Option<String>,
        /// 配额提取规则
        quotas: Vec<RegexQuotaRule>,
    },
}

/// JSON 模式的单条配额提取规则
///
/// 支持两种模式：
/// - 传统模式：`used` + `limit`（已用量 / 总配额），有进度条
/// - 余额模式：`remaining`（+ 可选 `used`），无进度条，仅展示余额
#[derive(Debug, Clone, Deserialize)]
pub struct JsonQuotaRule {
    /// 显示标签
    pub label: String,
    /// 已使用量的 JSON 路径（传统模式必填，余额模式可选）
    #[serde(default)]
    pub used: Option<String>,
    /// 总配额的 JSON 路径（传统模式必填）
    #[serde(default)]
    pub limit: Option<String>,
    /// 剩余额度的 JSON 路径（余额模式，与 limit 互斥）
    #[serde(default)]
    pub remaining: Option<String>,
    /// 配额类型
    #[serde(default = "default_quota_type")]
    pub quota_type: QuotaTypeDef,
    /// 详情文本的 JSON 路径（可选）
    #[serde(default)]
    pub detail: Option<String>,
    /// 可选除数：提取的数值会除以此值（用于单位换算，如 NewAPI 积分 → 美元）
    #[serde(default)]
    pub divisor: Option<f64>,
}

/// 正则模式的单条配额提取规则
#[derive(Debug, Clone, Deserialize)]
pub struct RegexQuotaRule {
    /// 显示标签
    pub label: String,
    /// 正则表达式
    pub pattern: String,
    /// used 值的 capture group 索引（从 1 开始）
    #[serde(default = "default_group_1")]
    pub used_group: usize,
    /// limit 值的 capture group 索引（从 1 开始）
    #[serde(default = "default_group_2")]
    pub limit_group: usize,
    /// 配额类型
    #[serde(default = "default_quota_type")]
    pub quota_type: QuotaTypeDef,
    /// 可选除数：提取的 used/limit 会除以此值（用于单位换算）
    #[serde(default)]
    pub divisor: Option<f64>,
}

/// YAML 中的配额类型枚举（映射到 models::QuotaType）
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum QuotaTypeDef {
    Session,
    Weekly,
    Credit,
    #[default]
    General,
}

fn default_group_1() -> usize {
    1
}
fn default_group_2() -> usize {
    2
}
fn default_quota_type() -> QuotaTypeDef {
    QuotaTypeDef::General
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_cli_provider() {
        let yaml = r#"
id: "myai:cli"
metadata:
  display_name: "My AI"
  brand_name: "MyCompany"
  dashboard_url: "https://myai.com/usage"
availability:
  type: cli_exists
  value: "myai"
source:
  type: cli
  command: "myai"
  args: ["usage", "--json"]
parser:
  format: regex
  quotas:
    - label: "Credits"
      pattern: 'Credits:\s*(\d+)/(\d+)'
      used_group: 1
      limit_group: 2
"#;
        let def: CustomProviderDef = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(def.id, "myai:cli");
        assert_eq!(def.metadata.display_name, "My AI");
        assert!(matches!(
            def.availability,
            AvailabilityDef::CliExists { .. }
        ));
        assert!(matches!(def.source, SourceDef::Cli { .. }));
        assert!(matches!(def.parser, ParserDef::Regex { .. }));
    }

    #[test]
    fn test_deserialize_http_provider() {
        let yaml = r#"
id: "custom:api"
metadata:
  display_name: "Custom API"
  brand_name: "Custom"
availability:
  type: env_var
  value: "CUSTOM_TOKEN"
source:
  type: http_post
  url: "https://api.custom.com/usage"
  auth:
    type: bearer_env
    env_var: "CUSTOM_TOKEN"
  headers:
    - name: "Origin"
      value: "https://custom.com"
  body: '{"scope":"coding"}'
parser:
  format: json
  account_email: "user.email"
  quotas:
    - label: "Weekly"
      used: "usage.used"
      limit: "usage.limit"
      quota_type: weekly
"#;
        let def: CustomProviderDef = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(def.id, "custom:api");
        assert!(matches!(def.availability, AvailabilityDef::EnvVar { .. }));
        assert!(matches!(def.source, SourceDef::HttpPost { .. }));
        if let ParserDef::Json { quotas, .. } = &def.parser {
            assert_eq!(quotas.len(), 1);
            assert!(matches!(quotas[0].quota_type, QuotaTypeDef::Weekly));
        } else {
            panic!("Expected JSON parser");
        }
    }

    #[test]
    fn test_deserialize_defaults() {
        let yaml = r#"
id: "min:cli"
metadata:
  display_name: "Minimal"
  brand_name: "Test"
availability:
  type: cli_exists
  value: "test"
source:
  type: cli
  command: "test"
parser:
  format: regex
  quotas:
    - label: "Usage"
      pattern: '(\d+)/(\d+)'
"#;
        let def: CustomProviderDef = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(def.metadata.icon, "🤖");
        assert_eq!(def.metadata.account_hint, "account");
        if let ParserDef::Regex { quotas, .. } = &def.parser {
            assert_eq!(quotas[0].used_group, 1);
            assert_eq!(quotas[0].limit_group, 2);
            assert!(matches!(quotas[0].quota_type, QuotaTypeDef::General));
        }
    }

    #[test]
    fn test_deserialize_json_with_divisor() {
        let yaml = r#"
id: "newapi:api"
metadata:
  display_name: "NewAPI"
  brand_name: "NewAPI"
availability:
  type: env_var
  value: "NEWAPI_API_KEY"
source:
  type: http_get
  url: "https://api.example.com/api/user/self"
parser:
  format: json
  quotas:
    - label: "Balance"
      used: "data.used_quota"
      limit: "data.quota"
      quota_type: credit
      divisor: 500000
"#;
        let def: CustomProviderDef = serde_yaml::from_str(yaml).unwrap();
        if let ParserDef::Json { quotas, .. } = &def.parser {
            assert_eq!(quotas[0].divisor, Some(500000.0));
            assert!(matches!(quotas[0].quota_type, QuotaTypeDef::Credit));
        } else {
            panic!("Expected JSON parser");
        }
    }

    #[test]
    fn test_deserialize_divisor_defaults_to_none() {
        let yaml = r#"
id: "test:api"
metadata:
  display_name: "Test"
  brand_name: "Test"
availability:
  type: env_var
  value: "TEST_KEY"
source:
  type: http_get
  url: "https://example.com/api"
parser:
  format: json
  quotas:
    - label: "Usage"
      used: "used"
      limit: "limit"
"#;
        let def: CustomProviderDef = serde_yaml::from_str(yaml).unwrap();
        if let ParserDef::Json { quotas, .. } = &def.parser {
            assert_eq!(quotas[0].divisor, None);
        } else {
            panic!("Expected JSON parser");
        }
    }

    #[test]
    fn test_deserialize_regex_with_divisor() {
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
parser:
  format: regex
  quotas:
    - label: "Credits"
      pattern: '(\d+)/(\d+)'
      divisor: 100
"#;
        let def: CustomProviderDef = serde_yaml::from_str(yaml).unwrap();
        if let ParserDef::Regex { quotas, .. } = &def.parser {
            assert_eq!(quotas[0].divisor, Some(100.0));
        } else {
            panic!("Expected Regex parser");
        }
    }

    #[test]
    fn test_deserialize_always_availability_and_bearer_auth() {
        let yaml = r#"
id: "newapi:api"
metadata:
  display_name: "NewAPI"
  brand_name: "NewAPI"
availability:
  type: always
source:
  type: http_get
  url: "https://example.com/api/user/self"
  auth:
    type: bearer
    token: "sk-test-123"
parser:
  format: json
  quotas:
    - label: "Balance"
      used: "data.used_quota"
      limit: "data.quota"
      quota_type: credit
      divisor: 500000
"#;
        let def: CustomProviderDef = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(def.availability, AvailabilityDef::Always));
        if let SourceDef::HttpGet { auth, .. } = &def.source {
            match auth.as_ref().unwrap() {
                AuthDef::Bearer { token } => assert_eq!(token, "sk-test-123"),
                _ => panic!("Expected Bearer auth"),
            }
        } else {
            panic!("Expected HttpGet source");
        }
    }

    #[test]
    fn test_deserialize_cookie_auth() {
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
    type: cookie
    value: "session=eyJhbGci...;cf_clearance=abc123"
parser:
  format: json
  quotas:
    - label: "Balance"
      used: "data.used_quota"
      limit: "data.quota"
"#;
        let def: CustomProviderDef = serde_yaml::from_str(yaml).unwrap();
        if let SourceDef::HttpGet { auth, .. } = &def.source {
            match auth.as_ref().unwrap() {
                AuthDef::Cookie { value } => {
                    assert_eq!(value, "session=eyJhbGci...;cf_clearance=abc123");
                }
                _ => panic!("Expected Cookie auth"),
            }
        } else {
            panic!("Expected HttpGet source");
        }
    }

    #[test]
    fn test_deserialize_session_token_auth() {
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
    type: session_token
    token: "eyJhbGciOiJIUzI1NiJ9"
parser:
  format: json
  quotas:
    - label: "Balance"
      used: "data.used_quota"
      limit: "data.quota"
"#;
        let def: CustomProviderDef = serde_yaml::from_str(yaml).unwrap();
        if let SourceDef::HttpGet { auth, .. } = &def.source {
            match auth.as_ref().unwrap() {
                AuthDef::SessionToken { token, cookie_name } => {
                    assert_eq!(token, "eyJhbGciOiJIUzI1NiJ9");
                    assert_eq!(cookie_name, "session"); // 默认值
                }
                _ => panic!("Expected SessionToken auth"),
            }
        } else {
            panic!("Expected HttpGet source");
        }
    }

    #[test]
    fn test_deserialize_session_token_custom_cookie_name() {
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
    type: session_token
    token: "abc123"
    cookie_name: "access_token"
parser:
  format: json
  quotas:
    - label: "Balance"
      used: "data.used_quota"
      limit: "data.quota"
"#;
        let def: CustomProviderDef = serde_yaml::from_str(yaml).unwrap();
        if let SourceDef::HttpGet { auth, .. } = &def.source {
            match auth.as_ref().unwrap() {
                AuthDef::SessionToken { token, cookie_name } => {
                    assert_eq!(token, "abc123");
                    assert_eq!(cookie_name, "access_token");
                }
                _ => panic!("Expected SessionToken auth"),
            }
        } else {
            panic!("Expected HttpGet source");
        }
    }
}
