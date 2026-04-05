use serde::Deserialize;

/// 自定义 Provider 的 YAML 定义（顶层结构）
#[derive(Debug, Clone, Deserialize)]
pub struct CustomProviderDef {
    /// 唯一标识符，如 "myai:cli"
    pub id: String,
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
    /// 从环境变量读取 Bearer token
    BearerEnv { env_var: String },
    /// 从环境变量读取自定义 header 值
    HeaderEnv { header: String, env_var: String },
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
#[derive(Debug, Clone, Deserialize)]
pub struct JsonQuotaRule {
    /// 显示标签
    pub label: String,
    /// 已使用量的 JSON 路径
    pub used: String,
    /// 总配额的 JSON 路径
    pub limit: String,
    /// 配额类型
    #[serde(default = "default_quota_type")]
    pub quota_type: QuotaTypeDef,
    /// 详情文本的 JSON 路径（可选）
    #[serde(default)]
    pub detail: Option<String>,
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
}
