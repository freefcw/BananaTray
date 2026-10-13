use crate::providers::common::secret::mask_secret_preview;
use crate::providers::{ProviderError, ProviderResult};
use serde_json::Value;
use std::path::{Path, PathBuf};

const MIN_KEY_CHARS: usize = 8;
const BASE_CANDIDATE_SCORE: i32 = 1;
const PROVIDER_MARKER_SCORE: i32 = 5;
const OFFICIAL_GATEWAY_SCORE: i32 = 3;
const MIN_HEURISTIC_SCORE: i32 = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GatewayPin {
    Global,
    China,
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TokenSource {
    ConfigFile,
    EnvVar,
    LocalApiKey,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ZCodeCredential {
    pub(super) token: String,
    pub(super) source: TokenSource,
    pub(super) gateway: GatewayPin,
}

impl ZCodeCredential {
    pub(super) fn masked(&self) -> String {
        mask_secret_preview(&self.token, "••••", |_| "••••••••".to_string())
    }
}

impl TokenSource {
    pub(super) fn log_label(self) -> &'static str {
        match self {
            Self::ConfigFile => "BananaTray settings",
            Self::EnvVar => "ZAI_API_KEY / GLM_API_KEY",
            Self::LocalApiKey => "ZCode config",
        }
    }
}

pub(super) enum LocalConfig<'a> {
    Unreadable,
    Contents(&'a str),
}

pub(super) struct AuthInputs<'a> {
    pub(super) configured_token: Option<&'a str>,
    pub(super) zai_api_key: Option<&'a str>,
    pub(super) glm_api_key: Option<&'a str>,
    pub(super) zai_region: Option<&'a str>,
    pub(super) api_host: Option<&'a str>,
    pub(super) local_files: &'a [LocalConfig<'a>],
}

pub(super) fn resolve_token(configured_token: Option<&str>) -> ProviderResult<ZCodeCredential> {
    let zai_api_key = std::env::var("ZAI_API_KEY").ok();
    let glm_api_key = std::env::var("GLM_API_KEY").ok();
    let zai_region = std::env::var("ZAI_REGION").ok();
    let api_host = std::env::var("Z_AI_API_HOST").ok();
    let local_files = if non_empty(configured_token).is_some()
        || non_empty(zai_api_key.as_deref()).is_some()
        || non_empty(glm_api_key.as_deref()).is_some()
    {
        Vec::new()
    } else {
        read_local_configs()
    };
    let local_refs: Vec<LocalConfig<'_>> = local_files
        .iter()
        .map(|file| match file {
            OwnedLocalConfig::Unreadable => LocalConfig::Unreadable,
            OwnedLocalConfig::Contents(body) => LocalConfig::Contents(body),
        })
        .collect();
    resolve_token_from_inputs(AuthInputs {
        configured_token,
        zai_api_key: zai_api_key.as_deref(),
        glm_api_key: glm_api_key.as_deref(),
        zai_region: zai_region.as_deref(),
        api_host: api_host.as_deref(),
        local_files: &local_refs,
    })
}

pub(super) fn resolve_token_from_inputs(inputs: AuthInputs<'_>) -> ProviderResult<ZCodeCredential> {
    let env_pin = gateway_pin_from_env(inputs.zai_region, inputs.api_host);
    if let Some(token) = non_empty(inputs.configured_token) {
        return Ok(ZCodeCredential {
            token: token.to_string(),
            source: TokenSource::ConfigFile,
            gateway: env_pin,
        });
    }
    if let Some(token) = non_empty(inputs.zai_api_key).or_else(|| non_empty(inputs.glm_api_key)) {
        return Ok(ZCodeCredential {
            token: token.to_string(),
            source: TokenSource::EnvVar,
            gateway: env_pin,
        });
    }

    let mut saw_unreadable = false;
    let mut saw_malformed = false;
    let mut saw_json = false;
    for file in inputs.local_files {
        match file {
            LocalConfig::Unreadable => saw_unreadable = true,
            LocalConfig::Contents(body) => match parse_config_json(body) {
                Err(()) => saw_malformed = true,
                Ok(hit) => {
                    saw_json = true;
                    if let Some(hit) = hit {
                        // 文件顺序是凭据优先级；只在同一份文件内比较多个候选。
                        return Ok(ZCodeCredential {
                            token: hit.token,
                            source: TokenSource::LocalApiKey,
                            gateway: hit.gateway.unwrap_or(env_pin),
                        });
                    }
                }
            },
        }
    }

    if saw_malformed && !saw_json {
        return Err(ProviderError::parse_failed("ZCode config"));
    }
    if saw_unreadable && !saw_json && !saw_malformed {
        return Err(ProviderError::unavailable("ZCode config unreadable"));
    }
    Err(ProviderError::config_missing(
        "zcode_api_key / ZAI_API_KEY / GLM_API_KEY / ~/.zcode",
    ))
}

pub(super) fn config_candidates(
    home: Option<&Path>,
    personal_provider_config: Option<&str>,
    data_base_dir: Option<&str>,
) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(path) = non_empty(personal_provider_config) {
        push_unique(&mut paths, PathBuf::from(path));
    }
    if let Some(dir) = non_empty(data_base_dir) {
        push_unique(
            &mut paths,
            PathBuf::from(dir).join("v2/provider_config.json"),
        );
    }
    if let Some(home) = home {
        push_unique(&mut paths, home.join(".zcode/v2/provider_config.json"));
        push_unique(&mut paths, home.join(".zcode/cli/provider_config.json"));
        push_unique(&mut paths, home.join(".zcode/cli/config.json"));
        push_unique(&mut paths, home.join(".zcode/v2/config.json"));
    }
    paths
}

enum OwnedLocalConfig {
    Unreadable,
    Contents(String),
}

fn read_local_configs() -> Vec<OwnedLocalConfig> {
    let personal = std::env::var("ZCODE_PERSONAL_PROVIDER_CONFIG_FILE").ok();
    let data_dir = std::env::var("ZCODE_DATA_BASE_DIR").ok();
    config_candidates(
        dirs::home_dir().as_deref(),
        personal.as_deref(),
        data_dir.as_deref(),
    )
    .into_iter()
    .filter(|path| path.exists())
    .map(|path| match std::fs::read_to_string(&path) {
        Ok(body) => OwnedLocalConfig::Contents(body),
        Err(_) => OwnedLocalConfig::Unreadable,
    })
    .collect()
}

struct LocalHit {
    token: String,
    gateway: Option<GatewayPin>,
    score: i32,
    candidate_order: usize,
}

fn prefer_hit(current: Option<LocalHit>, next: LocalHit) -> LocalHit {
    match current {
        Some(current)
            if (current.score, std::cmp::Reverse(current.candidate_order))
                >= (next.score, std::cmp::Reverse(next.candidate_order)) =>
        {
            current
        }
        _ => next,
    }
}

fn parse_config_json(body: &str) -> Result<Option<LocalHit>, ()> {
    let value: Value = serde_json::from_str(body).map_err(|_| ())?;
    let mut hits = Vec::new();
    walk(&value, "", &mut hits);
    Ok(hits
        .into_iter()
        .reduce(|best, hit| prefer_hit(Some(best), hit)))
}

fn walk(value: &Value, path: &str, hits: &mut Vec<LocalHit>) {
    match value {
        Value::Object(map) => {
            let own_base = string_field(map, &["baseURL", "baseUrl", "base_url"]);
            if let Some(token) = string_field(map, &["apiKey", "api_key"]) {
                let context = format!("{path} {}", context_text(map));
                if let Some(hit) = hit_from_key(&token, own_base.as_deref(), &context, hits.len()) {
                    hits.push(hit);
                }
            }
            for (key, child) in map {
                walk(child, &format!("{path}/{key}"), hits);
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                walk(item, &format!("{path}/{index}"), hits);
            }
        }
        _ => {}
    }
}

fn hit_from_key(token: &str, base: Option<&str>, context: &str, order: usize) -> Option<LocalHit> {
    let token = token.trim();
    if token.chars().count() < MIN_KEY_CHARS || token.to_ascii_lowercase().starts_with("enc:") {
        return None;
    }
    let mut score = BASE_CANDIDATE_SCORE;
    let context = context.to_ascii_lowercase();
    if has_provider_marker(&context) {
        score += PROVIDER_MARKER_SCORE;
    }
    let gateway = base.and_then(gateway_from_url);
    // 明确配置了第三方网关时不能把 key 发到官方网关。
    if base.is_some() && gateway.is_none() {
        return None;
    }
    if gateway.is_some() {
        score += OFFICIAL_GATEWAY_SCORE;
    }
    // 同一份 ZCode 配置里可能还有别的供应商 key。没有 Z.ai / BigModel 线索的不拿来用。
    if score < MIN_HEURISTIC_SCORE && gateway.is_none() {
        return None;
    }
    Some(LocalHit {
        token: token.to_string(),
        gateway,
        score,
        candidate_order: order,
    })
}

fn has_provider_marker(context: &str) -> bool {
    [
        "zai",
        "bigmodel",
        "zcode",
        "coding-plan",
        "coding_plan",
        "glm-coding-plan",
        "glm_coding_plan",
    ]
    .iter()
    .any(|marker| context.contains(marker))
}

fn context_text(map: &serde_json::Map<String, Value>) -> String {
    let mut parts = Vec::new();
    for (key, value) in map {
        if key.eq_ignore_ascii_case("apiKey") || key.eq_ignore_ascii_case("api_key") {
            parts.push(key.clone());
            continue;
        }
        parts.push(key.clone());
        if let Some(text) = value.as_str() {
            parts.push(text.to_string());
        }
    }
    parts.join(" ")
}

fn string_field(map: &serde_json::Map<String, Value>, names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        map.iter().find_map(|(key, value)| {
            if key.eq_ignore_ascii_case(name) {
                value.as_str().map(str::to_string)
            } else {
                None
            }
        })
    })
}

pub(super) fn gateway_pin_from_env(region: Option<&str>, host: Option<&str>) -> GatewayPin {
    if let Some(region) = non_empty(region) {
        match region.to_ascii_lowercase().as_str() {
            "china" | "cn" | "bigmodel" => return GatewayPin::China,
            "global" | "west" | "intl" | "international" => return GatewayPin::Global,
            _ => {}
        }
    }
    match non_empty(host).and_then(gateway_from_url) {
        Some(GatewayPin::Global) => GatewayPin::Global,
        Some(GatewayPin::China) => GatewayPin::China,
        _ => GatewayPin::Auto,
    }
}

fn gateway_from_url(url: &str) -> Option<GatewayPin> {
    let without_scheme = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    let host = without_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(without_scheme);
    let host = host.split(':').next().unwrap_or(host).to_ascii_lowercase();
    match host.as_str() {
        "api.z.ai" | "z.ai" => Some(GatewayPin::Global),
        "open.bigmodel.cn" | "bigmodel.cn" | "www.bigmodel.cn" => Some(GatewayPin::China),
        _ => None,
    }
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn push_unique(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.contains(&path) {
        paths.push(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs<'a>(
        configured: Option<&'a str>,
        zai: Option<&'a str>,
        glm: Option<&'a str>,
        region: Option<&'a str>,
        host: Option<&'a str>,
        files: &'a [LocalConfig<'a>],
    ) -> AuthInputs<'a> {
        AuthInputs {
            configured_token: configured,
            zai_api_key: zai,
            glm_api_key: glm,
            zai_region: region,
            api_host: host,
            local_files: files,
        }
    }

    #[test]
    fn settings_token_beats_env_and_local_files() {
        let files = [LocalConfig::Contents(
            r#"{"apiKey":"localkey123456","baseURL":"https://open.bigmodel.cn/api/anthropic"}"#,
        )];
        let credential = resolve_token_from_inputs(inputs(
            Some(" settings-key-123 "),
            Some("env-key-123456"),
            None,
            Some("china"),
            None,
            &files,
        ))
        .unwrap();
        assert_eq!(credential.token, "settings-key-123");
        assert_eq!(credential.source, TokenSource::ConfigFile);
        assert_eq!(credential.gateway, GatewayPin::China);
    }

    #[test]
    fn zai_env_beats_glm_env() {
        let credential = resolve_token_from_inputs(inputs(
            None,
            Some("zai-key-123456"),
            Some("glm-key-123456"),
            None,
            Some("https://open.bigmodel.cn"),
            &[],
        ))
        .unwrap();
        assert_eq!(credential.token, "zai-key-123456");
        assert_eq!(credential.source, TokenSource::EnvVar);
        assert_eq!(credential.gateway, GatewayPin::China);
    }

    #[test]
    fn local_config_prefers_coding_plan_key_and_its_gateway() {
        let body = r#"{
            "providers": {
                "custom": {"options": {"apiKey": "customkey123456"}},
                "builtin:zai-coding-plan": {
                    "options": {
                        "apiKey": "zaikey12345678",
                        "baseURL": "https://open.bigmodel.cn/api/anthropic"
                    }
                }
            }
        }"#;
        let files = [LocalConfig::Contents(body)];
        let credential =
            resolve_token_from_inputs(inputs(None, None, None, Some("global"), None, &files))
                .unwrap();
        assert_eq!(credential.token, "zaikey12345678");
        assert_eq!(credential.source, TokenSource::LocalApiKey);
        assert_eq!(credential.gateway, GatewayPin::China);
    }

    #[test]
    fn official_base_url_accepts_a_key_without_a_provider_name() {
        let files = [LocalConfig::Contents(
            r#"{"options":{"apiKey":"abcdefghijklmnop","baseURL":"https://api.z.ai/api/anthropic"}}"#,
        )];
        let credential =
            resolve_token_from_inputs(inputs(None, None, None, Some("china"), None, &files))
                .unwrap();
        assert_eq!(credential.token, "abcdefghijklmnop");
        assert_eq!(credential.gateway, GatewayPin::Global);
    }

    #[test]
    fn earlier_config_file_beats_more_specific_later_config() {
        let files = [
            LocalConfig::Contents(
                r#"{"options":{"apiKey":"preferred-key-123","baseURL":"https://api.z.ai/api/anthropic"}}"#,
            ),
            LocalConfig::Contents(
                r#"{"providers":{"zai":{"apiKey":"stale-key-123","baseURL":"https://api.z.ai/api/anthropic"}}}"#,
            ),
        ];
        let credential =
            resolve_token_from_inputs(inputs(None, None, None, None, None, &files)).unwrap();
        assert_eq!(credential.token, "preferred-key-123");
    }

    #[test]
    fn third_party_gateway_key_is_ignored() {
        let files = [LocalConfig::Contents(
            r#"{"providers":{"glm-relay":{"apiKey":"relay-key-123","baseURL":"https://relay.example.com/v1"}}}"#,
        )];
        let err =
            resolve_token_from_inputs(inputs(None, None, None, None, None, &files)).unwrap_err();
        assert!(matches!(err, ProviderError::ConfigMissing { .. }));
    }

    #[test]
    fn generic_glm_key_without_official_gateway_is_ignored() {
        let files = [LocalConfig::Contents(
            r#"{"providers":{"glm-relay":{"apiKey":"relay-key-123"}}}"#,
        )];
        let err =
            resolve_token_from_inputs(inputs(None, None, None, None, None, &files)).unwrap_err();
        assert!(matches!(err, ProviderError::ConfigMissing { .. }));
    }

    #[test]
    fn nested_key_does_not_inherit_parent_gateway() {
        let files = [LocalConfig::Contents(
            r#"{"baseURL":"https://api.z.ai","nested":{"apiKey":"nested-key-123"}}"#,
        )];
        let err =
            resolve_token_from_inputs(inputs(None, None, None, None, None, &files)).unwrap_err();
        assert!(matches!(err, ProviderError::ConfigMissing { .. }));
    }

    #[test]
    fn unrelated_plaintext_key_is_ignored() {
        let files = [LocalConfig::Contents(
            r#"{"providers":{"openai":{"options":{"apiKey":"sk-openai-key-12345"}}}}"#,
        )];
        let err =
            resolve_token_from_inputs(inputs(None, None, None, None, None, &files)).unwrap_err();
        assert!(matches!(err, ProviderError::ConfigMissing { .. }));
    }

    #[test]
    fn encrypted_session_is_not_a_token() {
        let files = [LocalConfig::Contents(
            r#"{"provider":{"zai":{"options":{"apiKey":"enc:v1:abcdefghijklmnop"}}}}"#,
        )];
        let err =
            resolve_token_from_inputs(inputs(None, None, None, None, None, &files)).unwrap_err();
        assert!(matches!(err, ProviderError::ConfigMissing { .. }));
    }

    #[test]
    fn malformed_config_without_another_source_is_parse_failed() {
        let files = [LocalConfig::Contents("{")];
        let err =
            resolve_token_from_inputs(inputs(None, None, None, None, None, &files)).unwrap_err();
        assert!(matches!(err, ProviderError::ParseFailed { .. }));
    }

    #[test]
    fn unreadable_config_is_unavailable() {
        let files = [LocalConfig::Unreadable];
        let err =
            resolve_token_from_inputs(inputs(None, None, None, None, None, &files)).unwrap_err();
        assert!(matches!(err, ProviderError::Unavailable { .. }));
    }

    #[test]
    fn candidate_paths_follow_zcode_overrides_then_home() {
        let paths = config_candidates(
            Some(Path::new("/home/user")),
            Some("/tmp/provider_config.json"),
            Some("/data/zcode"),
        );
        assert_eq!(
            paths,
            vec![
                PathBuf::from("/tmp/provider_config.json"),
                PathBuf::from("/data/zcode/v2/provider_config.json"),
                PathBuf::from("/home/user/.zcode/v2/provider_config.json"),
                PathBuf::from("/home/user/.zcode/cli/provider_config.json"),
                PathBuf::from("/home/user/.zcode/cli/config.json"),
                PathBuf::from("/home/user/.zcode/v2/config.json"),
            ]
        );
    }

    #[test]
    fn masked_preview_does_not_slice_bytes() {
        let credential = ZCodeCredential {
            token: "测试令牌abcdWXYZ".to_string(),
            source: TokenSource::ConfigFile,
            gateway: GatewayPin::Auto,
        };
        assert_eq!(credential.masked(), "测试令牌••••WXYZ");
    }
}
