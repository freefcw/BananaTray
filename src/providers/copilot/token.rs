use log::debug;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Token 来源类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopilotTokenSource {
    ConfigFile,
    EnvVar,
    CopilotOAuth,
    CopilotCli,
    None,
}

impl CopilotTokenSource {
    pub fn log_label(&self) -> &'static str {
        match self {
            Self::ConfigFile => "config file",
            Self::CopilotOAuth => "Copilot OAuth",
            Self::CopilotCli => "Copilot CLI (Keychain)",
            Self::EnvVar => "GITHUB_TOKEN env",
            Self::None => "none",
        }
    }
}

pub struct CopilotTokenStatus {
    pub token: Option<String>,
    pub source: CopilotTokenSource,
}

impl CopilotTokenStatus {
    pub fn masked(&self) -> Option<String> {
        self.token.as_ref().map(|t| {
            if t.len() <= 8 {
                "••••••••".to_string()
            } else {
                format!("{}••••{}", &t[..4], &t[t.len() - 4..])
            }
        })
    }
}

struct TokenCache {
    last_resolve: Option<Instant>,
    cached_oauth_token: Option<String>,
    cached_cli_token: Option<String>,
}

/// 进程级 token 缓存。
///
/// 使用 `static Mutex` 而非实例字段是因为 `define_unit_provider!` 生成零字段结构体，
/// 且 OAuth token 在进程生命周期内本身就是全局状态（所有刷新周期共享同一份凭据）。
/// 测试中可能存在并发竞争，但因为 `resolve_token` 的缓存不变量仅是"最近 5 秒内读取过"，
/// 竞争不会导致错误行为。
static TOKEN_CACHE: Mutex<TokenCache> = Mutex::new(TokenCache {
    last_resolve: None,
    cached_oauth_token: None,
    cached_cli_token: None,
});

const CACHE_DURATION: Duration = Duration::from_secs(5);

/// 解析 Copilot token，按优先级依次尝试：
///
/// 1. memory_token — 用户在设置界面手动配置（显式·应用内）
/// 2. GITHUB_TOKEN 环境变量（显式·系统级）
/// 3. ~/.config/github-copilot/ JSON 文件（隐式·VSCode 扩展自动检测）
/// 4. macOS Keychain copilot-cli（隐式·CLI 自动检测）
pub fn resolve_token(memory_token: Option<&str>) -> CopilotTokenStatus {
    debug!(target: "providers", "resolve_token: memory_token={:?}", memory_token.map(|t| if t.len() > 8 { &t[..8] } else { t }));

    // ① 用户手动配置的 token（最高优先级）
    if let Some(t) = memory_token.filter(|s| !s.is_empty()) {
        debug!(target: "providers", "resolve_token: → ConfigFile (from memory, len={})", t.len());
        return CopilotTokenStatus {
            token: Some(t.to_string()),
            source: CopilotTokenSource::ConfigFile,
        };
    }

    // ② 环境变量（显式设置，优先于隐式自动检测）
    if let Some(t) = std::env::var("GITHUB_TOKEN").ok().filter(|t| !t.is_empty()) {
        debug!(target: "providers", "resolve_token: → EnvVar (len={})", t.len());
        return CopilotTokenStatus {
            token: Some(t),
            source: CopilotTokenSource::EnvVar,
        };
    }

    // ③④ 以下为隐式自动检测来源，使用缓存避免频繁 I/O 和进程 fork
    let now = Instant::now();
    let mut cache = TOKEN_CACHE.lock().unwrap();

    let should_refresh = cache.last_resolve.is_none()
        || now.duration_since(cache.last_resolve.unwrap()) > CACHE_DURATION;

    if should_refresh {
        cache.cached_oauth_token = read_copilot_oauth_token();
        cache.cached_cli_token = read_copilot_cli_keychain_token();
        cache.last_resolve = Some(now);
        debug!(target: "providers", "resolve_token: cache refreshed (oauth={}, cli={})",
            cache.cached_oauth_token.is_some(), cache.cached_cli_token.is_some());
    }

    // ③ VSCode Copilot 扩展 OAuth token
    if let Some(t) = cache.cached_oauth_token.clone() {
        debug!(target: "providers", "resolve_token: → CopilotOAuth (cached, len={})", t.len());
        return CopilotTokenStatus {
            token: Some(t),
            source: CopilotTokenSource::CopilotOAuth,
        };
    }

    // ④ copilot-cli Keychain token
    if let Some(t) = cache.cached_cli_token.clone() {
        debug!(target: "providers", "resolve_token: → CopilotCli (cached, len={})", t.len());
        return CopilotTokenStatus {
            token: Some(t),
            source: CopilotTokenSource::CopilotCli,
        };
    }

    debug!(target: "providers", "resolve_token: → None (no token found)");
    CopilotTokenStatus {
        token: None,
        source: CopilotTokenSource::None,
    }
}

/// 从 VSCode Copilot 扩展的配置文件中读取 OAuth token。
///
/// 扫描 `~/.config/github-copilot/hosts.json` 和 `apps.json`，
/// 查找包含 `github.com` 的条目中的 `oauth_token` 字段。
fn read_copilot_oauth_token() -> Option<String> {
    let home = dirs::home_dir()?;
    let copilot_dir = home.join(".config").join("github-copilot");

    for filename in &["hosts.json", "apps.json"] {
        let path = copilot_dir.join(filename);
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Some(token) = extract_oauth_token_from_json(&content) {
                return Some(token);
            }
        }
    }
    None
}

/// 从 Copilot 扩展的 JSON 内容中提取 oauth_token。
///
/// JSON 格式为 `{ "github.com": { "oauth_token": "gho_..." }, ... }`。
fn extract_oauth_token_from_json(content: &str) -> Option<String> {
    let json: serde_json::Value = serde_json::from_str(content).ok()?;
    let obj = json.as_object()?;

    for (key, value) in obj {
        if key.contains("github.com") {
            if let Some(token) = value
                .get("oauth_token")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                return Some(token.to_string());
            }
        }
    }
    None
}

/// 从 macOS 钥匙串中读取 copilot-cli 存储的 OAuth token。
///
/// copilot-cli 使用 `security` 命令行工具访问 Keychain，
/// 服务名称为 `copilot-cli`。
#[cfg(target_os = "macos")]
fn read_copilot_cli_keychain_token() -> Option<String> {
    use std::process::Command;

    let output = Command::new("security")
        .args(["find-generic-password", "-s", "copilot-cli", "-w"])
        .output()
        .ok()?;

    if !output.status.success() {
        debug!(target: "providers", "read_copilot_cli_keychain_token: security command failed (status={})", output.status);
        return None;
    }

    let token = String::from_utf8(output.stdout).ok()?;
    let token = token.trim();

    if token.is_empty() {
        None
    } else {
        debug!(target: "providers", "read_copilot_cli_keychain_token: found token (len={})", token.len());
        Some(token.to_string())
    }
}

/// 非 macOS 平台不支持 Keychain 读取
#[cfg(not(target_os = "macos"))]
fn read_copilot_cli_keychain_token() -> Option<String> {
    None
}

#[cfg(test)]
pub(crate) fn set_test_cache(oauth: Option<&str>, cli: Option<&str>) {
    let mut cache = TOKEN_CACHE.lock().unwrap();
    cache.cached_oauth_token = oauth.map(str::to_string);
    cache.cached_cli_token = cli.map(str::to_string);
    cache.last_resolve = Some(Instant::now());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_masked_short_token() {
        let status = CopilotTokenStatus {
            token: Some("1234567".to_string()),
            source: CopilotTokenSource::EnvVar,
        };
        assert_eq!(status.masked().as_deref(), Some("••••••••"));
    }

    #[test]
    fn test_masked_long_token() {
        let status = CopilotTokenStatus {
            token: Some("abcdefgh12345678".to_string()),
            source: CopilotTokenSource::EnvVar,
        };
        assert_eq!(status.masked().as_deref(), Some("abcd••••5678"));
    }

    #[test]
    fn test_masked_gho_token() {
        let status = CopilotTokenStatus {
            token: Some("gho_YmUSQinMfsSRXKEGKPBP".to_string()),
            source: CopilotTokenSource::CopilotCli,
        };
        assert_eq!(status.masked().as_deref(), Some("gho_••••KPBP"));
    }

    #[test]
    fn test_masked_none_token() {
        let status = CopilotTokenStatus {
            token: None,
            source: CopilotTokenSource::None,
        };
        assert_eq!(status.masked(), None);
    }

    #[test]
    fn test_resolve_memory_token_has_priority() {
        let status = resolve_token(Some("ghp_test_123456"));
        assert!(matches!(status.source, CopilotTokenSource::ConfigFile));
        assert_eq!(status.token.as_deref(), Some("ghp_test_123456"));
    }

    #[test]
    fn test_resolve_empty_memory_token_skipped() {
        let status = resolve_token(Some(""));
        // 空字符串不应被视为有效 token，应 fallback 到后续来源
        assert!(!matches!(status.source, CopilotTokenSource::ConfigFile));
    }

    #[test]
    fn test_copilot_token_source_log_labels() {
        assert_eq!(CopilotTokenSource::ConfigFile.log_label(), "config file");
        assert_eq!(
            CopilotTokenSource::CopilotOAuth.log_label(),
            "Copilot OAuth"
        );
        assert_eq!(
            CopilotTokenSource::CopilotCli.log_label(),
            "Copilot CLI (Keychain)"
        );
        assert_eq!(CopilotTokenSource::EnvVar.log_label(), "GITHUB_TOKEN env");
        assert_eq!(CopilotTokenSource::None.log_label(), "none");
    }

    #[test]
    fn test_copilot_token_source_equality() {
        assert_eq!(
            CopilotTokenSource::CopilotCli,
            CopilotTokenSource::CopilotCli
        );
        assert_ne!(
            CopilotTokenSource::CopilotCli,
            CopilotTokenSource::CopilotOAuth
        );
        assert_ne!(CopilotTokenSource::CopilotCli, CopilotTokenSource::None);
    }

    // ── extract_oauth_token_from_json 测试 ──

    #[test]
    fn test_extract_oauth_token_hosts_json() {
        let json = r#"{"github.com": {"oauth_token": "gho_abc123456789"}}"#;
        assert_eq!(
            extract_oauth_token_from_json(json).as_deref(),
            Some("gho_abc123456789")
        );
    }

    #[test]
    fn test_extract_oauth_token_with_host_prefix() {
        // hosts.json 中的 key 可能带有 https:// 前缀
        let json = r#"{"https://github.com": {"oauth_token": "gho_xyz"}}"#;
        assert_eq!(
            extract_oauth_token_from_json(json).as_deref(),
            Some("gho_xyz")
        );
    }

    #[test]
    fn test_extract_oauth_token_empty_value() {
        let json = r#"{"github.com": {"oauth_token": ""}}"#;
        assert_eq!(extract_oauth_token_from_json(json), None);
    }

    #[test]
    fn test_extract_oauth_token_missing_field() {
        let json = r#"{"github.com": {"user": "test"}}"#;
        assert_eq!(extract_oauth_token_from_json(json), None);
    }

    #[test]
    fn test_extract_oauth_token_no_github_key() {
        let json = r#"{"gitlab.com": {"oauth_token": "glpat_123"}}"#;
        assert_eq!(extract_oauth_token_from_json(json), None);
    }

    #[test]
    fn test_extract_oauth_token_invalid_json() {
        assert_eq!(extract_oauth_token_from_json("not json"), None);
        assert_eq!(extract_oauth_token_from_json(""), None);
    }

    #[test]
    fn test_extract_oauth_token_multiple_hosts() {
        // 多个 host 条目时，应返回包含 github.com 的那个
        let json = r#"{
            "gitlab.com": {"oauth_token": "wrong"},
            "github.com": {"oauth_token": "correct"}
        }"#;
        assert_eq!(
            extract_oauth_token_from_json(json).as_deref(),
            Some("correct")
        );
    }
}
