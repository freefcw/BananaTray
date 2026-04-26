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
    let def: CustomProviderDef = serde_yml::from_str(yaml).unwrap();
    assert_eq!(def.id, "myai:cli");
    assert_eq!(def.metadata.display_name, "My AI");
    assert!(matches!(
        def.availability,
        AvailabilityDef::CliExists { .. }
    ));
    assert!(matches!(def.source, SourceDef::Cli { .. }));
    assert!(matches!(def.parser, Some(ParserDef::Regex { .. })));
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
    let def: CustomProviderDef = serde_yml::from_str(yaml).unwrap();
    assert_eq!(def.id, "custom:api");
    assert!(matches!(def.availability, AvailabilityDef::EnvVar { .. }));
    assert!(matches!(def.source, SourceDef::HttpPost { .. }));
    if let Some(ParserDef::Json { quotas, .. }) = &def.parser {
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
    let def: CustomProviderDef = serde_yml::from_str(yaml).unwrap();
    assert_eq!(def.metadata.icon, "");
    assert_eq!(def.metadata.account_hint, "account");
    if let Some(ParserDef::Regex { quotas, .. }) = &def.parser {
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
    let def: CustomProviderDef = serde_yml::from_str(yaml).unwrap();
    if let Some(ParserDef::Json { quotas, .. }) = &def.parser {
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
    let def: CustomProviderDef = serde_yml::from_str(yaml).unwrap();
    if let Some(ParserDef::Json { quotas, .. }) = &def.parser {
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
    let def: CustomProviderDef = serde_yml::from_str(yaml).unwrap();
    if let Some(ParserDef::Regex { quotas, .. }) = &def.parser {
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
    let def: CustomProviderDef = serde_yml::from_str(yaml).unwrap();
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
    let def: CustomProviderDef = serde_yml::from_str(yaml).unwrap();
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
    let def: CustomProviderDef = serde_yml::from_str(yaml).unwrap();
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
    let def: CustomProviderDef = serde_yml::from_str(yaml).unwrap();
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

#[test]
fn test_deserialize_login_auth() {
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
    type: login
    login_url: "https://site.com/api/user/login"
    username: "admin"
    password: "123456"
parser:
  format: json
  quotas:
    - label: "Balance"
      used: "data.used_quota"
      limit: "data.quota"
      divisor: 500000
"#;
    let def: CustomProviderDef = serde_yml::from_str(yaml).unwrap();
    if let SourceDef::HttpGet { auth, .. } = &def.source {
        match auth.as_ref().unwrap() {
            AuthDef::Login {
                login_url,
                username,
                password,
                token_path,
            } => {
                assert_eq!(login_url, "https://site.com/api/user/login");
                assert_eq!(username, "admin");
                assert_eq!(password, "123456");
                assert_eq!(token_path, "data");
            }
            _ => panic!("Expected Login auth"),
        }
    } else {
        panic!("Expected HttpGet source");
    }
}

// ── Phase 3: new schema types ────────────────

#[test]
fn test_deserialize_placeholder_source() {
    let yaml = r#"
id: "opencode:cli"
metadata:
  display_name: "OpenCode"
  brand_name: "OpenCode"
availability:
  type: cli_exists
  value: "opencode"
source:
  type: placeholder
  reason: "No public API available for quota monitoring"
"#;
    let def: CustomProviderDef = serde_yml::from_str(yaml).unwrap();
    assert_eq!(def.id, "opencode:cli");
    assert!(matches!(def.source, SourceDef::Placeholder { .. }));
    assert!(def.parser.is_none());
}

#[test]
fn test_deserialize_file_json_match_availability() {
    let yaml = r#"
id: "vertex:config"
metadata:
  display_name: "Vertex AI"
  brand_name: "Google"
availability:
  type: file_json_match
  path: "~/.gemini/settings.json"
  json_path: "security.auth.selectedType"
  expected: "vertex-ai"
source:
  type: placeholder
  reason: "Shares Gemini quota"
"#;
    let def: CustomProviderDef = serde_yml::from_str(yaml).unwrap();
    if let AvailabilityDef::FileJsonMatch {
        path,
        json_path,
        expected,
    } = &def.availability
    {
        assert_eq!(path, "~/.gemini/settings.json");
        assert_eq!(json_path, "security.auth.selectedType");
        assert_eq!(expected, "vertex-ai");
    } else {
        panic!("Expected FileJsonMatch availability");
    }
}

#[test]
fn test_deserialize_dir_contains_availability() {
    let yaml = r#"
id: "kilo:ext"
metadata:
  display_name: "Kilo"
  brand_name: "KiloCode"
availability:
  type: dir_contains
  path: "~/.vscode/extensions"
  prefix: "kilocode.kilo-code"
source:
  type: placeholder
  reason: "No public API available"
"#;
    let def: CustomProviderDef = serde_yml::from_str(yaml).unwrap();
    if let AvailabilityDef::DirContains { path, prefix } = &def.availability {
        assert_eq!(path, "~/.vscode/extensions");
        assert_eq!(prefix, "kilocode.kilo-code");
    } else {
        panic!("Expected DirContains availability");
    }
}

#[test]
fn test_deserialize_file_token_auth() {
    let yaml = r#"
id: "codex-like:api"
metadata:
  display_name: "CodexLike"
  brand_name: "OpenAI"
availability:
  type: file_exists
  value: "~/.codex/auth.json"
source:
  type: http_get
  url: "https://api.example.com/usage"
  auth:
    type: file_token
    path: "~/.codex/auth.json"
    token_path: "tokens.access_token"
parser:
  format: json
  quotas:
    - label: "Usage"
      used: "usage.used"
      limit: "usage.limit"
"#;
    let def: CustomProviderDef = serde_yml::from_str(yaml).unwrap();
    if let SourceDef::HttpGet { auth, .. } = &def.source {
        match auth.as_ref().unwrap() {
            AuthDef::FileToken { path, token_path } => {
                assert_eq!(path, "~/.codex/auth.json");
                assert_eq!(token_path, "tokens.access_token");
            }
            _ => panic!("Expected FileToken auth"),
        }
    } else {
        panic!("Expected HttpGet source");
    }
}

#[test]
fn test_deserialize_preprocess_strip_ansi() {
    let yaml = r#"
id: "kiro-like:cli"
metadata:
  display_name: "KiroLike"
  brand_name: "AWS"
availability:
  type: cli_exists
  value: "kiro-cli"
source:
  type: cli
  command: "kiro-cli"
  args: ["usage"]
preprocess:
  - strip_ansi
parser:
  format: regex
  quotas:
    - label: "Usage"
      pattern: '(\d+)/(\d+)'
"#;
    let def: CustomProviderDef = serde_yml::from_str(yaml).unwrap();
    assert_eq!(def.preprocess.len(), 1);
    assert!(matches!(def.preprocess[0], PreprocessStep::StripAnsi));
}

#[test]
fn test_deserialize_preprocess_defaults_empty() {
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
    - label: "Usage"
      pattern: '(\d+)/(\d+)'
"#;
    let def: CustomProviderDef = serde_yml::from_str(yaml).unwrap();
    assert!(def.preprocess.is_empty());
}
