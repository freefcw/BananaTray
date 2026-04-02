use log::debug;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Token 来源类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopilotTokenSource {
    ConfigFile,
    CopilotOAuth,
    EnvVar,
    None,
}

impl CopilotTokenSource {
    pub fn log_label(&self) -> &'static str {
        match self {
            Self::ConfigFile => "config file",
            Self::CopilotOAuth => "Copilot OAuth",
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
});

const CACHE_DURATION: Duration = Duration::from_secs(5);

pub fn resolve_token(memory_token: Option<&str>) -> CopilotTokenStatus {
    debug!(target: "providers", "resolve_token: memory_token={:?}", memory_token.map(|t| if t.len() > 8 { &t[..8] } else { t }));

    if let Some(t) = memory_token.filter(|s| !s.is_empty()) {
        debug!(target: "providers", "resolve_token: → ConfigFile (from memory, len={})", t.len());
        return CopilotTokenStatus {
            token: Some(t.to_string()),
            source: CopilotTokenSource::ConfigFile,
        };
    }

    let now = Instant::now();
    let mut cache = TOKEN_CACHE.lock().unwrap();

    let should_refresh = cache.last_resolve.is_none()
        || now.duration_since(cache.last_resolve.unwrap()) > CACHE_DURATION;

    if should_refresh {
        cache.cached_oauth_token = read_copilot_oauth_token();
        cache.last_resolve = Some(now);
        debug!(target: "providers", "resolve_token: cache refreshed");
    }

    if let Some(t) = cache.cached_oauth_token.clone() {
        debug!(target: "providers", "resolve_token: → CopilotOAuth (cached, len={})", t.len());
        return CopilotTokenStatus {
            token: Some(t),
            source: CopilotTokenSource::CopilotOAuth,
        };
    }

    if let Some(t) = std::env::var("GITHUB_TOKEN").ok().filter(|t| !t.is_empty()) {
        debug!(target: "providers", "resolve_token: → EnvVar (len={})", t.len());
        return CopilotTokenStatus {
            token: Some(t),
            source: CopilotTokenSource::EnvVar,
        };
    }

    debug!(target: "providers", "resolve_token: → None (no token found)");
    CopilotTokenStatus {
        token: None,
        source: CopilotTokenSource::None,
    }
}

fn read_copilot_oauth_token() -> Option<String> {
    let home = dirs::home_dir()?;
    let copilot_dir = home.join(".config").join("github-copilot");

    for filename in &["hosts.json", "apps.json"] {
        let path = copilot_dir.join(filename);
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(obj) = json.as_object() {
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
                }
            }
        }
    }
    None
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
    fn test_resolve_memory_token_has_priority() {
        let status = resolve_token(Some("ghp_test_123456"));
        assert!(matches!(status.source, CopilotTokenSource::ConfigFile));
        assert_eq!(status.token.as_deref(), Some("ghp_test_123456"));
    }
}
