use super::*;

fn first_step(def: &CustomProviderDef) -> &PlanStepDef {
    def.plan
        .steps
        .first()
        .expect("test YAML should have a step")
}

#[test]
fn deserialize_cli_provider_plan() {
    let yaml = r#"
schema_version: 2
id: "myai:cli"
metadata:
  display_name: "My AI"
  brand_name: "MyCompany"
  dashboard_url: "https://myai.com/usage"
plan:
  mode: first_success
  steps:
    - name: cli
      availability:
        type: cli_exists
        value: "myai"
      source:
        type: cli
        command: "myai"
        args: ["usage", "--json"]
        timeout_ms: 12000
      preprocess:
        - strip_ansi
      parser:
        format: regex
        quotas:
          - label: "Credits"
            pattern: 'Credits:\s*(\d+)/(\d+)'
            used_group: 1
            limit_group: 2
"#;
    let def: CustomProviderDef = serde_norway::from_str(yaml).unwrap();

    assert_eq!(def.schema_version, 2);
    assert_eq!(def.id, "myai:cli");
    assert_eq!(def.metadata.display_name, "My AI");
    assert_eq!(def.plan.mode, PlanMode::FirstSuccess);
    let step = first_step(&def);
    assert_eq!(step.name, "cli");
    assert!(step.required);
    assert!(matches!(
        step.availability,
        Some(AvailabilityDef::CliExists { .. })
    ));
    match &step.source {
        SourceDef::Cli {
            command,
            args,
            timeout_ms,
        } => {
            assert_eq!(command, "myai");
            assert_eq!(args, &vec!["usage".to_string(), "--json".to_string()]);
            assert_eq!(*timeout_ms, Some(12000));
        }
        _ => panic!("expected CLI source"),
    }
    assert!(matches!(step.parser, Some(ParserDef::Regex { .. })));
    assert!(matches!(step.preprocess[0], PreprocessStep::StripAnsi));
}

#[test]
fn deserialize_http_provider_plan() {
    let yaml = r#"
schema_version: 2
id: "custom:api"
metadata:
  display_name: "Custom API"
  brand_name: "Custom"
plan:
  mode: merge
  steps:
    - name: usage
      availability:
        type: env_var
        value: "CUSTOM_TOKEN"
      source:
        type: http
        method: post
        url: "https://api.custom.com/usage"
        timeout_ms: 3000
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
    let def: CustomProviderDef = serde_norway::from_str(yaml).unwrap();
    assert_eq!(def.plan.mode, PlanMode::Merge);
    let step = first_step(&def);
    assert!(matches!(
        step.availability,
        Some(AvailabilityDef::EnvVar { .. })
    ));

    match &step.source {
        SourceDef::Http {
            method,
            url,
            timeout_ms,
            auth,
            headers,
            body,
        } => {
            assert_eq!(*method, HttpMethodDef::Post);
            assert_eq!(url, "https://api.custom.com/usage");
            assert_eq!(*timeout_ms, Some(3000));
            assert!(matches!(auth.as_ref().unwrap(), AuthDef::BearerEnv { .. }));
            assert_eq!(headers.len(), 1);
            assert_eq!(body.as_deref(), Some(r#"{"scope":"coding"}"#));
        }
        _ => panic!("expected HTTP source"),
    }

    if let Some(ParserDef::Json { quotas, .. }) = &step.parser {
        assert_eq!(quotas.len(), 1);
        assert!(matches!(quotas[0].quota_type, QuotaTypeDef::Weekly));
    } else {
        panic!("expected JSON parser");
    }
}

#[test]
fn deserialize_defaults() {
    let yaml = r#"
schema_version: 2
id: "min:cli"
metadata:
  display_name: "Minimal"
  brand_name: "Test"
plan:
  steps:
    - name: cli
      source:
        type: cli
        command: "test"
      parser:
        format: regex
        quotas:
          - label: "Usage"
            pattern: '(\d+)/(\d+)'
"#;
    let def: CustomProviderDef = serde_norway::from_str(yaml).unwrap();
    assert_eq!(def.metadata.icon, "");
    assert_eq!(def.metadata.account_hint, "account");
    assert_eq!(def.plan.mode, PlanMode::FirstSuccess);

    let step = first_step(&def);
    assert!(step.required);
    assert!(step.availability.is_none());
    if let Some(ParserDef::Regex { quotas, .. }) = &step.parser {
        assert_eq!(quotas[0].used_group, 1);
        assert_eq!(quotas[0].limit_group, 2);
        assert!(matches!(quotas[0].quota_type, QuotaTypeDef::General));
    } else {
        panic!("expected regex parser");
    }
}

#[test]
fn deserialize_json_balance_with_divisor() {
    let yaml = r#"
schema_version: 2
id: "newapi:api"
metadata:
  display_name: "NewAPI"
  brand_name: "NewAPI"
plan:
  steps:
    - name: api
      source:
        type: http
        url: "https://api.example.com/api/user/self"
      parser:
        format: json
        quotas:
          - label: "Balance"
            remaining: "data.quota"
            used: "data.used_quota"
            quota_type: credit
            divisor: 500000
"#;
    let def: CustomProviderDef = serde_norway::from_str(yaml).unwrap();
    if let Some(ParserDef::Json { quotas, .. }) = &first_step(&def).parser {
        assert_eq!(quotas[0].divisor, Some(500000.0));
        assert!(matches!(quotas[0].quota_type, QuotaTypeDef::Credit));
    } else {
        panic!("expected JSON parser");
    }
}

#[test]
fn deserialize_auth_variants() {
    let yaml = r#"
schema_version: 2
id: "auth:api"
metadata:
  display_name: "Auth API"
  brand_name: "Auth"
plan:
  steps:
    - name: bearer
      source:
        type: http
        url: "https://example.com/bearer"
        auth:
          type: bearer
          token: "sk-test-123"
      parser:
        format: json
        quotas:
          - label: "Usage"
            used: "used"
            limit: "limit"
    - name: cookie
      source:
        type: http
        url: "https://example.com/cookie"
        auth:
          type: cookie
          value: "session=abc;cf_clearance=xyz"
      parser:
        format: json
        quotas:
          - label: "Usage"
            used: "used"
            limit: "limit"
    - name: session
      source:
        type: http
        url: "https://example.com/session"
        auth:
          type: session_token
          token: "abc123"
          cookie_name: "access_token"
      parser:
        format: json
        quotas:
          - label: "Usage"
            used: "used"
            limit: "limit"
    - name: file
      source:
        type: http
        url: "https://example.com/file"
        auth:
          type: file_token
          path: "~/.codex/auth.json"
          token_path: "tokens.access_token"
      parser:
        format: json
        quotas:
          - label: "Usage"
            used: "used"
            limit: "limit"
    - name: login
      source:
        type: http
        url: "https://example.com/login"
        auth:
          type: login
          login_url: "https://example.com/api/user/login"
          username: "admin"
          password: "secret"
      parser:
        format: json
        quotas:
          - label: "Usage"
            used: "used"
            limit: "limit"
"#;
    let def: CustomProviderDef = serde_norway::from_str(yaml).unwrap();
    assert_eq!(def.plan.steps.len(), 5);

    let source_auth = |index: usize| match &def.plan.steps[index].source {
        SourceDef::Http { auth, .. } => auth.as_ref().unwrap(),
        _ => panic!("expected HTTP source"),
    };
    assert!(matches!(source_auth(0), AuthDef::Bearer { .. }));
    assert!(matches!(source_auth(1), AuthDef::Cookie { .. }));
    assert!(matches!(
        source_auth(2),
        AuthDef::SessionToken {
            cookie_name,
            ..
        } if cookie_name == "access_token"
    ));
    assert!(matches!(source_auth(3), AuthDef::FileToken { .. }));
    assert!(matches!(
        source_auth(4),
        AuthDef::Login {
            token_path,
            ..
        } if token_path == "data"
    ));
}

#[test]
fn deserialize_placeholder_step() {
    let yaml = r#"
schema_version: 2
id: "opencode:cli"
metadata:
  display_name: "OpenCode"
  brand_name: "OpenCode"
plan:
  steps:
    - name: detect
      availability:
        type: cli_exists
        value: "opencode"
      source:
        type: placeholder
        reason: "No public API available for quota monitoring"
"#;
    let def: CustomProviderDef = serde_norway::from_str(yaml).unwrap();
    let step = first_step(&def);
    assert!(matches!(step.source, SourceDef::Placeholder { .. }));
    assert!(step.parser.is_none());
}

#[test]
fn deserialize_file_json_and_dir_availability() {
    let yaml = r#"
schema_version: 2
id: "multi:detect"
metadata:
  display_name: "Multi"
  brand_name: "Multi"
plan:
  steps:
    - name: vertex
      availability:
        type: file_json_match
        path: "~/.gemini/settings.json"
        json_path: "security.auth.selectedType"
        expected: "vertex-ai"
      source:
        type: placeholder
        reason: "Shares Gemini quota"
    - name: kilo
      required: false
      availability:
        type: dir_contains
        path: "~/.vscode/extensions"
        prefix: "kilocode.kilo-code"
      source:
        type: placeholder
        reason: "No public API"
"#;
    let def: CustomProviderDef = serde_norway::from_str(yaml).unwrap();
    assert!(matches!(
        def.plan.steps[0].availability,
        Some(AvailabilityDef::FileJsonMatch { .. })
    ));
    assert!(matches!(
        def.plan.steps[1].availability,
        Some(AvailabilityDef::DirContains { .. })
    ));
    assert!(!def.plan.steps[1].required);
}
