pub mod settings_ui;

use super::AiProvider;
use crate::models::{ProviderKind, ProviderMetadata, QuotaInfo, QuotaType, RefreshData};
use crate::utils::http_client;
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use log::debug;
use rust_i18n::t;
use serde::Deserialize;

// ============================================================================
// Token 解析
// ============================================================================

/// Token 来源类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopilotTokenSource {
    /// 用户在配置文件中手动设置的 token
    ConfigFile,
    /// 从 GitHub Copilot 扩展的 OAuth 配置自动检测
    CopilotOAuth,
    /// 从 GITHUB_TOKEN 环境变量读取
    EnvVar,
    /// 没有找到任何 token
    None,
}

impl CopilotTokenSource {
    /// 用于日志的简短英文标签
    pub fn log_label(&self) -> &'static str {
        match self {
            Self::ConfigFile => "config file",
            Self::CopilotOAuth => "Copilot OAuth",
            Self::EnvVar => "GITHUB_TOKEN env",
            Self::None => "none",
        }
    }
}

/// Copilot Token 解析结果
pub struct CopilotTokenStatus {
    /// 有效的 token（可能来自内存/磁盘/环境变量）
    pub token: Option<String>,
    /// token 来源
    pub source: CopilotTokenSource,
}

impl CopilotTokenStatus {
    /// 返回脱敏后的 token 字符串
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

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 缓存结构：存储上次解析的结果和时间
struct TokenCache {
    last_resolve: Option<Instant>,
    cached_oauth_token: Option<String>,
}

static TOKEN_CACHE: Mutex<TokenCache> = Mutex::new(TokenCache {
    last_resolve: None,
    cached_oauth_token: None,
});

/// 缓存有效期：5秒（避免频繁的文件 I/O）
const CACHE_DURATION: Duration = Duration::from_secs(5);

/// 从多个来源解析 GitHub Token
///
/// 优先级：BananaTray 配置 > Copilot 扩展 OAuth > 环境变量 GITHUB_TOKEN
///
/// 使用基于时间的缓存（5秒有效期）来避免每次渲染都执行文件 I/O
pub fn resolve_token(memory_token: Option<&str>) -> CopilotTokenStatus {
    debug!(target: "providers", "resolve_token: memory_token={:?}", memory_token.map(|t| if t.len() > 8 { &t[..8] } else { t }));

    // 1. BananaTray 配置文件（settings.json 中的 github_token，启动时已加载到内存）
    // 这个不缓存，因为内存状态可能随时变化
    if let Some(t) = memory_token.filter(|s| !s.is_empty()) {
        debug!(target: "providers", "resolve_token: → ConfigFile (from memory, len={})", t.len());
        return CopilotTokenStatus {
            token: Some(t.to_string()),
            source: CopilotTokenSource::ConfigFile,
        };
    }

    // 2. 检查缓存是否有效
    let now = Instant::now();
    let mut cache = TOKEN_CACHE.lock().unwrap();

    let should_refresh = cache.last_resolve.is_none()
        || now.duration_since(cache.last_resolve.unwrap()) > CACHE_DURATION;

    if should_refresh {
        // 缓存过期，重新读取
        cache.cached_oauth_token = read_copilot_oauth_token();
        cache.last_resolve = Some(now);
        debug!(target: "providers", "resolve_token: cache refreshed");
    }

    // 3. 使用缓存的 OAuth token
    if let Some(t) = cache.cached_oauth_token.clone() {
        debug!(target: "providers", "resolve_token: → CopilotOAuth (cached, len={})", t.len());
        return CopilotTokenStatus {
            token: Some(t),
            source: CopilotTokenSource::CopilotOAuth,
        };
    }

    // 4. 环境变量（快速操作，不需要缓存）
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

/// 从 ~/.config/github-copilot/ 读取已有的 OAuth token
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

// ============================================================================
// AiProvider 实现
// ============================================================================

super::define_unit_provider!(CopilotProvider);

/// Copilot Internal API 响应结构
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct CopilotInternalResponse {
    copilot_plan: Option<String>,
    quota_snapshots: Option<QuotaSnapshots>,
}

#[derive(Debug, Deserialize)]
struct QuotaSnapshots {
    premium_interactions: Option<InteractionQuota>,
}

#[derive(Debug, Deserialize)]
struct InteractionQuota {
    entitlement: i32,
    remaining: i32,
    #[allow(dead_code)]
    percent_remaining: f64,
    unlimited: Option<bool>,
}

#[async_trait]
impl AiProvider for CopilotProvider {
    fn metadata(&self) -> ProviderMetadata {
        ProviderMetadata {
            kind: ProviderKind::Copilot,
            display_name: "Copilot".into(),
            brand_name: "GitHub".into(),
            icon_asset: "src/icons/provider-copilot.svg".into(),
            dashboard_url: "https://github.com/settings/copilot".into(),
            account_hint: "GitHub account".into(),
            source_label: "github api".into(),
        }
    }

    fn id(&self) -> &'static str {
        "copilot:api"
    }

    async fn is_available(&self) -> bool {
        let token_status = resolve_token(None);
        let available = token_status.token.is_some();
        debug!(target: "providers", "Copilot availability: {} (token source: {})",
            available, token_status.source.log_label());
        available
    }

    async fn refresh(&self) -> Result<RefreshData> {
        let token_status = resolve_token(None);

        let token = token_status.token
            .context("GitHub token not configured. Set github_token in settings, or GITHUB_TOKEN environment variable.")?;

        let auth_header = format!("Authorization: Bearer {}", token);
        let (body, status_code) = http_client::get_with_status(
            "https://api.github.com/copilot_internal/user",
            &[&auth_header, "Accept: application/json"],
        )?;

        debug!(target: "providers", "Copilot API response status: {}", status_code);

        match status_code.as_str() {
            "401" => bail!(
                "GitHub token is invalid or expired. Update your token in Settings → Providers."
            ),
            "403" => {
                bail!("Token lacks required permissions. Use a Classic PAT with 'copilot' scope.")
            }
            "404" => bail!(
                "Copilot not enabled for this account. Check your GitHub Copilot subscription."
            ),
            _ => {}
        }

        // 解析响应
        let resp: CopilotInternalResponse = serde_json::from_str(&body)
            .context("Failed to parse Copilot Internal API response.")?;

        debug!(target: "providers", "Copilot response parsed: plan={:?}",
            resp.copilot_plan);

        let plan = resp.copilot_plan.unwrap_or_else(|| "unknown".to_string());
        let plan_label = capitalize_first(&plan);

        // 检查是否有 premium_interactions 配额
        let quota = if let Some(snapshots) = resp.quota_snapshots {
            if let Some(interactions) = snapshots.premium_interactions {
                if interactions.unlimited.unwrap_or(false) {
                    QuotaInfo::with_details(
                        t!("quota.label.premium_requests", plan = plan_label).to_string(),
                        0.0,
                        0.0,
                        QuotaType::General,
                        Some(t!("quota.label.unlimited").to_string()),
                    )
                } else {
                    let used = (interactions.entitlement - interactions.remaining).max(0) as f64;
                    let limit = interactions.entitlement as f64;
                    QuotaInfo::with_details(
                        t!("quota.label.premium_requests", plan = plan_label).to_string(),
                        used,
                        limit,
                        QuotaType::Weekly,
                        None,
                    )
                }
            } else {
                QuotaInfo::with_details(
                    t!("quota.label.chat_completions", plan = plan_label).to_string(),
                    0.0,
                    0.0,
                    QuotaType::General,
                    Some(t!("quota.label.unlimited").to_string()),
                )
            }
        } else {
            bail!("No quota data found in Copilot API response.");
        };

        Ok(RefreshData::with_account(
            vec![quota],
            None, // GitHub Copilot 不提供账户邮箱
            Some(plan_label),
        ))
    }
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}
