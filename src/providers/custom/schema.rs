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
    /// 响应解析规则（placeholder source 时可省略）
    pub parser: Option<ParserDef>,
    /// 响应预处理管道（解析前执行，可选）
    #[serde(default)]
    pub preprocess: Vec<PreprocessStep>,
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
    String::new()
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
    /// 检查 JSON 文件中特定路径的值是否匹配
    ///
    /// 覆盖场景：VertexAI 检查 `~/.gemini/settings.json` 中 `security.auth.selectedType == "vertex-ai"`
    FileJsonMatch {
        /// 文件路径（支持 ~ 展开）
        path: String,
        /// JSON 点分路径
        json_path: String,
        /// 期望值
        expected: String,
    },
    /// 检查目录中是否存在匹配前缀的子项
    ///
    /// 覆盖场景：Kilo 检查 `~/.vscode/extensions/` 下是否有 `kilocode.kilo-code` 前缀的目录
    DirContains {
        /// 目录路径（支持 ~ 展开）
        path: String,
        /// 子项名称前缀
        prefix: String,
    },
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
    /// 占位 Provider：不获取数据，直接返回不可用错误
    ///
    /// 覆盖场景：OpenCode / Kilo / VertexAI 等只需检测安装但无法监控的 Provider
    Placeholder {
        /// 不可用的原因说明
        reason: String,
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
    /// 从本地 JSON 文件读取 token（自动作为 Bearer token 发送）
    ///
    /// 覆盖场景：Codex 从 `~/.codex/auth.json` → `tokens.access_token` 读取 OAuth token
    FileToken {
        /// 文件路径（支持 ~ 展开）
        path: String,
        /// JSON 点分路径提取 token 值
        token_path: String,
    },
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

/// 响应预处理步骤
///
/// 在将原始响应传给 parser 之前执行的清洗操作。
/// 覆盖场景：Kiro CLI 输出包含 ANSI 转义码和 Unicode 进度条字符。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreprocessStep {
    /// 移除 ANSI 转义序列和 Unicode 进度条字符
    StripAnsi,
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
#[path = "schema_tests.rs"]
mod tests;
