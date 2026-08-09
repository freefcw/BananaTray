use crate::models::FailureAdvice;
use crate::providers::{ProviderError, ProviderResult};
use log::debug;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const OAUTH_EXPIRY_BUFFER_MS: u64 = 20_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ClineTokenSource {
    ConfigFile,
    EnvVar,
    LocalApiKey,
    LocalOAuth,
    None,
}

pub(super) struct ClineTokenStatus {
    pub(super) token: Option<String>,
    pub(super) source: ClineTokenSource,
}

impl ClineTokenStatus {
    pub(super) fn masked(&self) -> Option<String> {
        self.token.as_ref().map(|token| {
            crate::providers::common::secret::mask_secret_preview(token, "••••", |_| {
                "••••••••".to_string()
            })
        })
    }
}

impl ClineTokenSource {
    pub(super) fn log_label(self) -> &'static str {
        match self {
            Self::ConfigFile => "BananaTray settings",
            Self::EnvVar => "CLINE_API_KEY env",
            Self::LocalApiKey => "Cline providers.json API key",
            Self::LocalOAuth => "Cline providers.json OAuth",
            Self::None => "none",
        }
    }
}

pub(super) fn resolve_token_from_inputs<F>(
    configured_token: Option<&str>,
    env_token: Option<&str>,
    read_local: F,
) -> ProviderResult<ClineTokenStatus>
where
    F: FnOnce() -> ProviderResult<ClineTokenStatus>,
{
    if let Some(token) = non_empty(configured_token) {
        return Ok(ClineTokenStatus {
            token: Some(token.to_string()),
            source: ClineTokenSource::ConfigFile,
        });
    }
    if let Some(token) = non_empty(env_token) {
        return Ok(ClineTokenStatus {
            token: Some(token.to_string()),
            source: ClineTokenSource::EnvVar,
        });
    }
    read_local()
}

pub(super) fn resolve_token(configured_token: Option<&str>) -> ProviderResult<ClineTokenStatus> {
    let env_token = std::env::var("CLINE_API_KEY").ok();
    resolve_token_from_inputs(configured_token, env_token.as_deref(), read_local_token)
}

#[derive(Deserialize)]
struct ProvidersFile {
    providers: HashMap<String, ProviderEntry>,
}

#[derive(Deserialize)]
struct ProviderEntry {
    settings: ProviderSettings,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderSettings {
    api_key: Option<String>,
    auth: Option<AuthSettings>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuthSettings {
    api_key: Option<String>,
    access_token: Option<String>,
    expires_at: Option<u64>,
}

#[derive(Deserialize)]
struct JwtClaims {
    exp: u64,
}

pub(super) fn settings_path_from_sources(
    explicit_path: Option<&str>,
    data_dir: Option<&str>,
    cline_dir: Option<&str>,
    home_dir: Option<&Path>,
) -> Option<PathBuf> {
    if let Some(path) = non_empty(explicit_path) {
        return Some(PathBuf::from(path));
    }
    if let Some(path) = non_empty(data_dir) {
        return Some(PathBuf::from(path).join("settings/providers.json"));
    }
    if let Some(path) = non_empty(cline_dir) {
        return Some(PathBuf::from(path).join("data/settings/providers.json"));
    }
    home_dir.map(|home| home.join(".cline/data/settings/providers.json"))
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

#[cfg(test)]
pub(super) fn parse_providers_json(body: &str, now_ms: u64) -> ProviderResult<Option<String>> {
    Ok(parse_providers_json_status(body, now_ms)?.token)
}

fn parse_providers_json_status(body: &str, now_ms: u64) -> ProviderResult<ClineTokenStatus> {
    let file: ProvidersFile = serde_json::from_str(body)
        .map_err(|_| ProviderError::parse_failed("Cline providers.json"))?;
    let Some(settings) = file
        .providers
        .get("cline")
        .or_else(|| file.providers.get("cline-pass"))
        .map(|entry| &entry.settings)
    else {
        return Ok(empty_status());
    };

    if let Some(api_key) = settings
        .api_key
        .as_deref()
        .or_else(|| settings.auth.as_ref()?.api_key.as_deref())
        .filter(|token| !token.trim().is_empty())
    {
        return Ok(ClineTokenStatus {
            token: Some(api_key.trim().to_string()),
            source: ClineTokenSource::LocalApiKey,
        });
    }

    let Some(auth) = settings.auth.as_ref() else {
        return Ok(empty_status());
    };
    let Some(access_token) = auth
        .access_token
        .as_deref()
        .filter(|token| !token.trim().is_empty())
    else {
        return Ok(empty_status());
    };
    let normalized_token = normalize_workos_token(access_token);
    let expires_at = auth
        .expires_at
        .filter(|expires_at| *expires_at > 0)
        .or_else(|| jwt_expiry_ms(&normalized_token));
    if expires_at
        .is_none_or(|expires_at| expires_at <= now_ms.saturating_add(OAUTH_EXPIRY_BUFFER_MS))
    {
        return Err(ProviderError::session_expired(Some(
            FailureAdvice::LoginApp {
                app: "Cline".to_string(),
            },
        )));
    }

    Ok(ClineTokenStatus {
        token: Some(normalized_token),
        source: ClineTokenSource::LocalOAuth,
    })
}

fn normalize_workos_token(token: &str) -> String {
    let token = token.trim();
    if token.to_ascii_lowercase().starts_with("workos:") {
        token.to_string()
    } else {
        format!("workos:{token}")
    }
}

fn jwt_expiry_ms(token: &str) -> Option<u64> {
    let claims: JwtClaims = crate::providers::common::jwt::decode_payload(token).ok()?;
    claims.exp.checked_mul(1_000).filter(|expiry| *expiry > 0)
}

fn empty_status() -> ClineTokenStatus {
    ClineTokenStatus {
        token: None,
        source: ClineTokenSource::None,
    }
}

fn current_settings_path() -> Option<PathBuf> {
    let explicit_path = std::env::var("CLINE_PROVIDER_SETTINGS_PATH").ok();
    let data_dir = std::env::var("CLINE_DATA_DIR").ok();
    let cline_dir = std::env::var("CLINE_DIR").ok();
    settings_path_from_sources(
        explicit_path.as_deref(),
        data_dir.as_deref(),
        cline_dir.as_deref(),
        dirs::home_dir().as_deref(),
    )
}

fn read_local_token() -> ProviderResult<ClineTokenStatus> {
    let Some(path) = current_settings_path() else {
        return Ok(empty_status());
    };
    let body = match std::fs::read_to_string(&path) {
        Ok(body) => body,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(empty_status()),
        Err(error) => {
            return Err(ProviderError::unavailable(&format!(
                "failed to read {}: {error}",
                path.display()
            )));
        }
    };
    let now_ms = crate::utils::time_utils::now_epoch_secs().max(0) as u64 * 1_000;
    let status = parse_providers_json_status(&body, now_ms)?;
    debug!(
        target: "providers",
        "cline-pass: local credential source: {}",
        status.source.log_label()
    );
    Ok(status)
}
