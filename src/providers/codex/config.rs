//! Codex `~/.codex/config.toml` 读取与 usage URL 归一化。
//!
//! 与 CodexBar `CodexOAuthUsageFetcher` 中的 `resolveUsageURL` / `parseChatGPTBaseURL` /
//! `normalizeChatGPTBaseURL` 保持等价行为：
//! - `chatgpt_base_url` 是 Codex CLI config.toml 的顶层 key；只解析顶层单行赋值
//! - 主机为 `chatgpt.com` / `chat.openai.com` 且不含 `/backend-api` 时自动补齐
//! - 任何解析或 I/O 失败都静默回退到默认 URL
//!
//! 设计：把"读文件 / 读环境变量"的 I/O 与纯文本处理拆开，
//! 纯函数可以直接注入 `Option<&str>` 进行单元测试，不依赖 tmp 文件系统。

use std::path::PathBuf;

const DEFAULT_BASE_URL: &str = "https://chatgpt.com/backend-api";
/// `/backend-api` 前缀下的 usage path。
const CHATGPT_USAGE_PATH: &str = "/wham/usage";
/// 自托管或非 chatgpt.com 域下的 usage path（与 CodexBar `codexUsagePath` 一致）。
const CODEX_USAGE_PATH: &str = "/wham/usage";

/// 解析并返回最终的 usage API URL。读取 `$CODEX_HOME/config.toml`（默认
/// `~/.codex/config.toml`），任何阶段失败都回退到默认 URL。
pub(super) fn resolve_usage_url() -> String {
    let contents = load_config_contents();
    let base = resolve_base_url(contents.as_deref());
    assemble_usage_url(&base)
}

/// 纯函数：从可选的 config 内容推导有效 base URL（含归一化）。
fn resolve_base_url(config_contents: Option<&str>) -> String {
    let raw = config_contents
        .and_then(parse_base_url)
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
    normalize_base_url(&raw)
}

/// 纯函数：扫描 TOML 文本，匹配顶层 `chatgpt_base_url = "..."`。
///
/// 简化策略（与 CodexBar 等价）：
/// - 仅识别第一处匹配，找到就返回
/// - 忽略 `#` 后注释
/// - 支持双引号或单引号包裹的值
/// - 不处理嵌套 section / inline table / 多行字符串：Codex CLI 把该 key 规定在顶层
fn parse_base_url(contents: &str) -> Option<String> {
    for raw_line in contents.lines() {
        let without_comment = raw_line.split('#').next().unwrap_or("");
        let line = without_comment.trim();
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "chatgpt_base_url" {
            continue;
        }
        let unquoted = strip_quotes(value.trim()).trim();
        if unquoted.is_empty() {
            return None;
        }
        return Some(unquoted.to_string());
    }
    None
}

/// 去掉首尾的成对引号（`"..."` 或 `'...'`），不成对则原样返回。
fn strip_quotes(s: &str) -> &str {
    let bytes = s.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
    {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

/// 纯函数：归一化 base URL。
///
/// **只对官方 ChatGPT 主机（`chatgpt.com` / `chat.openai.com`）自动追加
/// `/backend-api`**。判断基于解析出的主机名做严格相等比较（含 `www.` 子
/// 域），避免把 `https://chatgpt.company.internal`、
/// `https://chat.openai.com.proxy.local` 这类前缀恰好相同的自托管网关
/// 误判为官方域名而追加错误 path。
fn normalize_base_url(value: &str) -> String {
    let trimmed = value.trim();
    let candidate = if trimmed.is_empty() {
        DEFAULT_BASE_URL
    } else {
        trimmed
    };
    let mut s = candidate.to_string();
    while s.ends_with('/') {
        s.pop();
    }
    if is_official_chatgpt_host(&s) && !s.contains("/backend-api") {
        s.push_str("/backend-api");
    }
    s
}

/// 判定给定 URL 的主机是否是 ChatGPT 官方域名。
///
/// 官方域名集合（与 CodexBar `normalizeChatGPTBaseURL` 等价）：
/// - `chatgpt.com`
/// - `www.chatgpt.com`
/// - `chat.openai.com`
/// - `www.chat.openai.com`
fn is_official_chatgpt_host(url: &str) -> bool {
    let Some(host) = extract_host(url) else {
        return false;
    };
    let host_lower = host.to_ascii_lowercase();
    matches!(
        host_lower.as_str(),
        "chatgpt.com" | "www.chatgpt.com" | "chat.openai.com" | "www.chat.openai.com"
    )
}

/// 纯函数：从形如 `scheme://host[:port][/path][?q][#f]` 的 URL 中抽出 host。
///
/// 不引入 `url` crate（项目刻意保持零新增依赖）。支持的边界：
/// - 缺 scheme、缺 host → None
/// - 带 userinfo（`user:pass@host`）→ 取 `@` 之后的 host
/// - 带端口（`host:port`）→ 剥掉端口
fn extract_host(url: &str) -> Option<&str> {
    let (_scheme, rest) = url.split_once("://")?;
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    if authority.is_empty() {
        return None;
    }
    // strip userinfo
    let host_with_port = match authority.rsplit_once('@') {
        Some((_user, h)) => h,
        None => authority,
    };
    // strip port
    let host = match host_with_port.split_once(':') {
        Some((h, _port)) => h,
        None => host_with_port,
    };
    if host.is_empty() {
        None
    } else {
        Some(host)
    }
}

/// 纯函数：根据 base 是否含 `/backend-api` 选择 path 并拼接最终 URL。
fn assemble_usage_url(base: &str) -> String {
    let path = if base.contains("/backend-api") {
        CHATGPT_USAGE_PATH
    } else {
        CODEX_USAGE_PATH
    };
    format!("{}{}", base, path)
}

/// I/O：读取 `$CODEX_HOME/config.toml`，回退到 `~/.codex/config.toml`。
/// 文件不存在或读取失败均返回 None，由调用方走默认 URL。
fn load_config_contents() -> Option<String> {
    let path = config_path()?;
    std::fs::read_to_string(path).ok()
}

/// 薄 I/O 胶水：从 process env / dirs 拼出实际路径，纯函数部分下沉到
/// [`build_config_path`] 供单测使用。
fn config_path() -> Option<PathBuf> {
    build_config_path(std::env::var("CODEX_HOME").ok(), dirs::home_dir())
}

/// 纯函数：按 CODEX_HOME / home 推导 config.toml 路径。
///
/// - `codex_home` 为非空字符串 → 以其为根
/// - `codex_home` 为空 / 纯空白 / None → 回退到 `home_dir/.codex`
/// - `home_dir` 也为 None → 返回 None（调用方走默认 URL）
fn build_config_path(codex_home: Option<String>, home_dir: Option<PathBuf>) -> Option<PathBuf> {
    let env_home = codex_home
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let root = match env_home {
        Some(s) => PathBuf::from(s),
        None => home_dir?.join(".codex"),
    };
    Some(root.join("config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ────────────────────────────────────────────────────────────────────────
    // parse_base_url：行扫描语法
    // ────────────────────────────────────────────────────────────────────────

    #[test]
    fn parse_base_url_double_quoted() {
        let toml = r#"chatgpt_base_url = "https://example.com/backend-api""#;
        assert_eq!(
            parse_base_url(toml).as_deref(),
            Some("https://example.com/backend-api")
        );
    }

    #[test]
    fn parse_base_url_single_quoted() {
        let toml = "chatgpt_base_url = 'https://example.com'";
        assert_eq!(parse_base_url(toml).as_deref(), Some("https://example.com"));
    }

    #[test]
    fn parse_base_url_unquoted() {
        let toml = "chatgpt_base_url = https://example.com";
        assert_eq!(parse_base_url(toml).as_deref(), Some("https://example.com"));
    }

    #[test]
    fn parse_base_url_strips_inline_comment() {
        let toml = r#"chatgpt_base_url = "https://example.com" # self-hosted"#;
        assert_eq!(parse_base_url(toml).as_deref(), Some("https://example.com"));
    }

    #[test]
    fn parse_base_url_skips_full_line_comment() {
        let toml = "# chatgpt_base_url = \"https://commented.example.com\"\n\
                    chatgpt_base_url = \"https://real.example.com\"";
        assert_eq!(
            parse_base_url(toml).as_deref(),
            Some("https://real.example.com")
        );
    }

    #[test]
    fn parse_base_url_returns_none_when_missing() {
        assert!(parse_base_url("model = \"gpt-5\"\nother = 1").is_none());
    }

    #[test]
    fn parse_base_url_returns_none_for_empty_value() {
        assert!(parse_base_url("chatgpt_base_url = \"\"").is_none());
        assert!(parse_base_url("chatgpt_base_url = \"   \"").is_none());
    }

    #[test]
    fn parse_base_url_returns_first_match_only() {
        let toml = "chatgpt_base_url = \"https://first.example\"\n\
                    chatgpt_base_url = \"https://second.example\"";
        assert_eq!(
            parse_base_url(toml).as_deref(),
            Some("https://first.example")
        );
    }

    #[test]
    fn parse_base_url_ignores_blank_and_section_headers() {
        // 简化策略：section 头不被识别也不会误当成 key。
        let toml = "\n[experimental]\nchatgpt_base_url = \"https://example.com\"\n";
        assert_eq!(parse_base_url(toml).as_deref(), Some("https://example.com"));
    }

    #[test]
    fn parse_base_url_tolerates_extra_whitespace() {
        let toml = "   chatgpt_base_url   =   \"https://x.example\"   ";
        assert_eq!(parse_base_url(toml).as_deref(), Some("https://x.example"));
    }

    // ────────────────────────────────────────────────────────────────────────
    // normalize_base_url
    // ────────────────────────────────────────────────────────────────────────

    #[test]
    fn normalize_strips_trailing_slashes() {
        assert_eq!(
            normalize_base_url("https://example.com///"),
            "https://example.com"
        );
    }

    #[test]
    fn normalize_appends_backend_api_for_chatgpt_host() {
        assert_eq!(
            normalize_base_url("https://chatgpt.com"),
            "https://chatgpt.com/backend-api"
        );
        assert_eq!(
            normalize_base_url("https://chat.openai.com/"),
            "https://chat.openai.com/backend-api"
        );
    }

    #[test]
    fn normalize_keeps_existing_backend_api_path() {
        assert_eq!(
            normalize_base_url("https://chatgpt.com/backend-api"),
            "https://chatgpt.com/backend-api"
        );
        assert_eq!(
            normalize_base_url("https://chatgpt.com/backend-api/"),
            "https://chatgpt.com/backend-api"
        );
    }

    #[test]
    fn normalize_does_not_touch_self_hosted_host() {
        assert_eq!(
            normalize_base_url("https://gateway.example.com"),
            "https://gateway.example.com"
        );
    }

    /// 回归：prefix 相同但不是官方域名的自托管网关**必须**原样返回，
    /// 不能因为 `starts_with("https://chatgpt.com")` 被误追加 `/backend-api`。
    #[test]
    fn normalize_does_not_mistake_prefix_similar_hosts_for_chatgpt() {
        // 公司内网（前缀 chatgpt.com 但其实是 chatgpt.company.internal）
        assert_eq!(
            normalize_base_url("https://chatgpt.company.internal"),
            "https://chatgpt.company.internal"
        );
        // 反向代理/CDN（前缀 chat.openai.com 但其实是 .proxy.local）
        assert_eq!(
            normalize_base_url("https://chat.openai.com.proxy.local"),
            "https://chat.openai.com.proxy.local"
        );
        // 带路径 / 端口 / 大小写混用：均不应被当作官方域名
        assert_eq!(
            normalize_base_url("https://chatgpt.com.evil.example/api"),
            "https://chatgpt.com.evil.example/api"
        );
        assert_eq!(
            normalize_base_url("https://chatgpt.company.internal:8443"),
            "https://chatgpt.company.internal:8443"
        );
    }

    #[test]
    fn normalize_handles_www_aliases_for_chatgpt_host() {
        assert_eq!(
            normalize_base_url("https://www.chatgpt.com"),
            "https://www.chatgpt.com/backend-api"
        );
        assert_eq!(
            normalize_base_url("https://www.chat.openai.com/"),
            "https://www.chat.openai.com/backend-api"
        );
    }

    #[test]
    fn normalize_is_case_insensitive_on_host() {
        // DNS 主机名对大小写不敏感；URL host 用大小写混写也应识别为官方。
        assert_eq!(
            normalize_base_url("https://ChatGPT.com"),
            "https://ChatGPT.com/backend-api"
        );
    }

    #[test]
    fn normalize_handles_chatgpt_host_with_port() {
        // 端口号不应影响主机名判定。
        assert_eq!(
            normalize_base_url("https://chatgpt.com:443"),
            "https://chatgpt.com:443/backend-api"
        );
    }

    #[test]
    fn normalize_empty_falls_back_to_default() {
        assert_eq!(normalize_base_url(""), DEFAULT_BASE_URL);
        assert_eq!(normalize_base_url("   "), DEFAULT_BASE_URL);
    }

    // ────────────────────────────────────────────────────────────────────────
    // assemble_usage_url
    // ────────────────────────────────────────────────────────────────────────

    #[test]
    fn assemble_uses_wham_usage_path() {
        assert_eq!(
            assemble_usage_url("https://chatgpt.com/backend-api"),
            "https://chatgpt.com/backend-api/wham/usage"
        );
        assert_eq!(
            assemble_usage_url("https://gateway.example.com"),
            "https://gateway.example.com/wham/usage"
        );
    }

    // ────────────────────────────────────────────────────────────────────────
    // resolve_base_url：组合行为
    // ────────────────────────────────────────────────────────────────────────

    #[test]
    fn resolve_base_url_uses_default_when_no_config() {
        assert_eq!(resolve_base_url(None), DEFAULT_BASE_URL);
    }

    #[test]
    fn resolve_base_url_uses_default_when_key_missing() {
        let toml = "model = \"gpt-5\"";
        assert_eq!(resolve_base_url(Some(toml)), DEFAULT_BASE_URL);
    }

    #[test]
    fn resolve_base_url_self_hosted_passthrough() {
        let toml = "chatgpt_base_url = \"https://gateway.example.com/v1\"";
        assert_eq!(
            resolve_base_url(Some(toml)),
            "https://gateway.example.com/v1"
        );
    }

    #[test]
    fn resolve_base_url_chatgpt_host_gets_backend_api_appended() {
        let toml = "chatgpt_base_url = \"https://chatgpt.com\"";
        assert_eq!(
            resolve_base_url(Some(toml)),
            "https://chatgpt.com/backend-api"
        );
    }

    // ────────────────────────────────────────────────────────────────────────
    // 端到端纯函数：config 内容 → 最终 URL
    // ────────────────────────────────────────────────────────────────────────

    fn url_from_config(contents: Option<&str>) -> String {
        assemble_usage_url(&resolve_base_url(contents))
    }

    #[test]
    fn end_to_end_default_when_no_config() {
        assert_eq!(
            url_from_config(None),
            "https://chatgpt.com/backend-api/wham/usage"
        );
    }

    #[test]
    fn end_to_end_self_hosted_gateway() {
        let toml = "chatgpt_base_url = \"https://gateway.example.com/v1\"";
        assert_eq!(
            url_from_config(Some(toml)),
            "https://gateway.example.com/v1/wham/usage"
        );
    }

    #[test]
    fn end_to_end_chatgpt_host_without_backend_api_gets_completed() {
        let toml = "chatgpt_base_url = \"https://chatgpt.com\"";
        assert_eq!(
            url_from_config(Some(toml)),
            "https://chatgpt.com/backend-api/wham/usage"
        );
    }

    #[test]
    fn end_to_end_chat_openai_host_alias() {
        let toml = "chatgpt_base_url = \"https://chat.openai.com\"";
        assert_eq!(
            url_from_config(Some(toml)),
            "https://chat.openai.com/backend-api/wham/usage"
        );
    }

    // ────────────────────────────────────────────────────────────────────────
    // strip_quotes：边界情况
    // ────────────────────────────────────────────────────────────────────────

    #[test]
    fn strip_quotes_handles_single_char() {
        assert_eq!(strip_quotes("\""), "\"");
        assert_eq!(strip_quotes("'"), "'");
    }

    #[test]
    fn strip_quotes_passes_unquoted() {
        assert_eq!(strip_quotes("plain"), "plain");
    }

    #[test]
    fn strip_quotes_does_not_strip_unbalanced() {
        assert_eq!(strip_quotes("\"unbalanced"), "\"unbalanced");
        assert_eq!(strip_quotes("unbalanced\""), "unbalanced\"");
    }

    // ────────────────────────────────────────────────────────────────────────
    // extract_host / is_official_chatgpt_host：精确主机识别
    // ────────────────────────────────────────────────────────────────────────

    #[test]
    fn extract_host_basic() {
        assert_eq!(extract_host("https://chatgpt.com"), Some("chatgpt.com"));
        assert_eq!(
            extract_host("https://chatgpt.com/backend-api"),
            Some("chatgpt.com")
        );
        assert_eq!(extract_host("https://chatgpt.com:443"), Some("chatgpt.com"));
        assert_eq!(
            extract_host("https://chatgpt.com/p?x=1#f"),
            Some("chatgpt.com")
        );
    }

    #[test]
    fn extract_host_strips_userinfo() {
        assert_eq!(
            extract_host("https://user:pass@chatgpt.com/x"),
            Some("chatgpt.com")
        );
    }

    #[test]
    fn extract_host_handles_prefix_similar() {
        assert_eq!(
            extract_host("https://chatgpt.company.internal"),
            Some("chatgpt.company.internal")
        );
        assert_eq!(
            extract_host("https://chat.openai.com.proxy.local/x"),
            Some("chat.openai.com.proxy.local")
        );
    }

    #[test]
    fn extract_host_returns_none_for_invalid() {
        assert!(extract_host("not-a-url").is_none());
        assert!(extract_host("https:///path-only").is_none());
        assert!(extract_host("").is_none());
    }

    #[test]
    fn is_official_chatgpt_host_covers_known_aliases() {
        assert!(is_official_chatgpt_host("https://chatgpt.com"));
        assert!(is_official_chatgpt_host("https://www.chatgpt.com/"));
        assert!(is_official_chatgpt_host("https://chat.openai.com/any"));
        assert!(is_official_chatgpt_host("https://www.chat.openai.com"));
        // 大小写不敏感
        assert!(is_official_chatgpt_host("https://CHATGPT.com"));
    }

    #[test]
    fn is_official_chatgpt_host_rejects_prefix_similar_hosts() {
        // 与 CodexBar 行为一致：这些**不**是官方域名，必须返回 false。
        assert!(!is_official_chatgpt_host(
            "https://chatgpt.company.internal"
        ));
        assert!(!is_official_chatgpt_host(
            "https://chat.openai.com.proxy.local"
        ));
        assert!(!is_official_chatgpt_host(
            "https://chatgpt.com.evil.example"
        ));
        assert!(!is_official_chatgpt_host("https://gateway.example.com"));
    }

    // ────────────────────────────────────────────────────────────────────────
    // build_config_path：纯函数路径拼装（避免 env mutation，无并发风险）
    // ────────────────────────────────────────────────────────────────────────

    #[test]
    fn build_config_path_uses_codex_home_when_set() {
        let path = build_config_path(
            Some("/tmp/custom-codex-home".to_string()),
            Some(PathBuf::from("/Users/whoever")),
        );
        assert_eq!(
            path,
            Some(PathBuf::from("/tmp/custom-codex-home/config.toml"))
        );
    }

    #[test]
    fn build_config_path_codex_home_takes_precedence_over_home_dir() {
        // 两者都在时以 CODEX_HOME 为准。
        let path = build_config_path(
            Some("/codex/elsewhere".to_string()),
            Some(PathBuf::from("/home/user")),
        );
        assert_eq!(path, Some(PathBuf::from("/codex/elsewhere/config.toml")));
    }

    #[test]
    fn build_config_path_blank_codex_home_falls_back_to_home_dir() {
        let path = build_config_path(Some("   ".to_string()), Some(PathBuf::from("/home/user")));
        assert_eq!(path, Some(PathBuf::from("/home/user/.codex/config.toml")));
    }

    #[test]
    fn build_config_path_empty_codex_home_falls_back_to_home_dir() {
        let path = build_config_path(Some(String::new()), Some(PathBuf::from("/home/user")));
        assert_eq!(path, Some(PathBuf::from("/home/user/.codex/config.toml")));
    }

    #[test]
    fn build_config_path_none_codex_home_falls_back_to_home_dir() {
        let path = build_config_path(None, Some(PathBuf::from("/home/user")));
        assert_eq!(path, Some(PathBuf::from("/home/user/.codex/config.toml")));
    }

    #[test]
    fn build_config_path_returns_none_when_both_missing() {
        // 极端环境：既没有 CODEX_HOME 也拿不到 home dir。
        // resolve_usage_url 会因此走默认 URL。
        assert_eq!(build_config_path(None, None), None);
        assert_eq!(build_config_path(Some("   ".into()), None), None);
    }
}
