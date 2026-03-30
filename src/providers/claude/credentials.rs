//! Claude OAuth 凭证管理
//!
//! 从 ~/.claude/.credentials.json 加载凭证，支持 Token 刷新检查。

use anyhow::{Context, Result};
use log::debug;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// OAuth 凭证
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeOAuthCredentials {
    pub access_token: String,
    pub refresh_token: Option<String>,
    /// 过期时间（毫秒时间戳）
    pub expires_at: Option<f64>,
    pub subscription_type: Option<String>,
}

impl ClaudeOAuthCredentials {
    /// 从 ~/.claude/.credentials.json 加载凭证
    pub fn load() -> Result<Self> {
        let path = Self::credentials_path();

        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("无法读取凭证文件: {:?}", path))?;

        let json: serde_json::Value =
            serde_json::from_str(&content).with_context(|| "无法解析凭证文件 JSON")?;

        let oauth = json
            .get("claudeAiOauth")
            .with_context(|| "凭证文件中缺少 claudeAiOauth 字段")?;

        let raw_access_token = oauth
            .get("accessToken")
            .and_then(|v| v.as_str())
            .with_context(|| "缺少 accessToken")?;

        let access_token = raw_access_token.trim().to_string();
        if access_token.is_empty() {
            anyhow::bail!("accessToken 为空");
        }

        Ok(ClaudeOAuthCredentials {
            access_token,
            refresh_token: oauth
                .get("refreshToken")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            expires_at: oauth.get("expiresAt").and_then(|v| v.as_f64()),
            subscription_type: oauth
                .get("subscriptionType")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        })
    }

    /// 尝试加载凭证，失败返回 None
    pub fn try_load() -> Option<Self> {
        Self::load().ok()
    }

    /// Token 是否需要刷新（过期或 5 分钟内过期）
    pub fn needs_refresh(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            let now_ms = Self::current_time_ms();
            let buffer_ms = 5.0 * 60.0 * 1000.0;
            now_ms + buffer_ms >= expires_at
        } else {
            true
        }
    }

    /// 应用刷新响应到凭证
    pub fn apply_refresh(&mut self, response: &TokenRefreshResponse) {
        self.access_token = response.access_token.clone();
        if let Some(ref new_refresh) = response.refresh_token {
            self.refresh_token = Some(new_refresh.clone());
        }
        if let Some(expires_in) = response.expires_in {
            let now_ms = Self::current_time_ms();
            self.expires_at = Some(now_ms + expires_in as f64 * 1000.0);
        }
    }

    /// 凭证文件路径
    pub fn credentials_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".claude")
            .join(".credentials.json")
    }

    /// 当前时间（毫秒时间戳）
    pub fn current_time_ms() -> f64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as f64
    }
}

/// Token 刷新响应
#[derive(Debug, Deserialize)]
pub struct TokenRefreshResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: Option<i64>,
}

/// 刷新 Token
pub fn refresh_oauth_token(refresh_token: &str) -> Result<TokenRefreshResponse> {
    const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
    const SCOPES: &str = "user:profile user:inference user:sessions:claude_code";
    const REFRESH_URL: &str = "https://platform.claude.com/v1/oauth/token";

    let body = serde_json::json!({
        "grant_type": "refresh_token",
        "refresh_token": refresh_token,
        "client_id": CLIENT_ID,
        "scope": SCOPES
    });

    let response = crate::utils::http_client::post_json(
        REFRESH_URL,
        &["Content-Type: application/json"],
        &body.to_string(),
    )?;

    let refresh_response: TokenRefreshResponse =
        serde_json::from_str(&response).with_context(|| "无法解析 Token 刷新响应")?;

    Ok(refresh_response)
}

/// 原子保存凭证到文件（先写临时文件再 rename，不会破坏原文件）
pub fn save_credentials_atomic(creds: &ClaudeOAuthCredentials) -> Result<()> {
    let path = ClaudeOAuthCredentials::credentials_path();

    // 读取现有文件，解析失败则报错（不覆盖损坏文件）
    let existing =
        std::fs::read_to_string(&path).with_context(|| format!("无法读取凭证文件: {:?}", path))?;
    let mut json: serde_json::Value =
        serde_json::from_str(&existing).with_context(|| "凭证文件 JSON 格式损坏，拒绝覆写")?;

    json["claudeAiOauth"] = serde_json::json!({
        "accessToken": creds.access_token,
        "refreshToken": creds.refresh_token,
        "expiresAt": creds.expires_at,
        "subscriptionType": creds.subscription_type,
    });

    let serialized = serde_json::to_string_pretty(&json)?;

    // 原子写入：先写临时文件再 rename
    let tmp_path = path.with_extension("json.tmp");
    std::fs::write(&tmp_path, &serialized)
        .with_context(|| format!("无法写入临时凭证文件: {:?}", tmp_path))?;

    // 在 Unix 上设置权限 0600
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        let _ = std::fs::set_permissions(&tmp_path, perms);
    }

    std::fs::rename(&tmp_path, &path)
        .with_context(|| format!("无法原子替换凭证文件: {:?}", path))?;

    debug!("Claude: 凭证文件已更新");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_needs_refresh_when_expired() {
        let creds = ClaudeOAuthCredentials {
            access_token: "test".to_string(),
            refresh_token: None,
            expires_at: Some(ClaudeOAuthCredentials::current_time_ms() - 60_000.0),
            subscription_type: None,
        };
        assert!(creds.needs_refresh());
    }

    #[test]
    fn test_needs_refresh_when_within_buffer() {
        let creds = ClaudeOAuthCredentials {
            access_token: "test".to_string(),
            refresh_token: None,
            expires_at: Some(ClaudeOAuthCredentials::current_time_ms() + 3.0 * 60_000.0),
            subscription_type: None,
        };
        assert!(creds.needs_refresh());
    }

    #[test]
    fn test_needs_refresh_when_not_needed() {
        let creds = ClaudeOAuthCredentials {
            access_token: "test".to_string(),
            refresh_token: None,
            expires_at: Some(ClaudeOAuthCredentials::current_time_ms() + 10.0 * 60_000.0),
            subscription_type: None,
        };
        assert!(!creds.needs_refresh());
    }

    #[test]
    fn test_needs_refresh_when_no_expiry() {
        let creds = ClaudeOAuthCredentials {
            access_token: "test".to_string(),
            refresh_token: None,
            expires_at: None,
            subscription_type: None,
        };
        assert!(creds.needs_refresh());
    }

    #[test]
    fn test_apply_refresh() {
        let mut creds = ClaudeOAuthCredentials {
            access_token: "old".to_string(),
            refresh_token: Some("old_rt".to_string()),
            expires_at: None,
            subscription_type: None,
        };

        let response = TokenRefreshResponse {
            access_token: "new_token".to_string(),
            refresh_token: Some("new_rt".to_string()),
            expires_in: Some(3600),
        };

        creds.apply_refresh(&response);

        assert_eq!(creds.access_token, "new_token");
        assert_eq!(creds.refresh_token, Some("new_rt".to_string()));
        assert!(creds.expires_at.is_some());
        assert!(!creds.needs_refresh());
    }
}
