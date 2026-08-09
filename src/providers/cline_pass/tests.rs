use super::{
    cline_pass_auth_header, parse_providers_json, parse_usage_response, resolve_token_from_inputs,
    settings_path_from_sources, ClinePassProvider, ClineTokenSource, USAGE_URL,
};
use crate::models::{
    AppSettings, ProviderKind, QuotaDetailSpec, QuotaLabelSpec, QuotaType, SettingsCapability,
    TokenEditMode,
};
use crate::providers::{AiProvider, ProviderCapabilities, ProviderError};
use base64::Engine;
use std::path::{Path, PathBuf};

const NOW_MS: u64 = 1_800_000_000_000;

fn jwt_with_exp(exp_secs: u64) -> String {
    let payload =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(format!(r#"{{"exp":{exp_secs}}}"#));
    format!("header.{payload}.signature")
}

#[test]
fn provider_declares_cline_pass_identity_and_token_input() {
    let provider = ClinePassProvider::new();
    let descriptor = provider.descriptor();

    assert_eq!(descriptor.id, "cline-pass:api");
    assert_eq!(descriptor.metadata.kind, ProviderKind::ClinePass);
    assert_eq!(descriptor.metadata.display_name, "ClinePass");
    let SettingsCapability::TokenInput(capability) = provider.settings_capability() else {
        panic!("ClinePass must expose token input settings");
    };
    assert_eq!(capability.credential_key, "cline_api_key");
}

#[test]
fn configured_cline_api_key_is_editable_in_token_input() {
    let provider = ClinePassProvider::new();
    let mut settings = AppSettings::default();
    settings
        .provider
        .credentials
        .set_credential("cline_api_key", "cline-configured-key".to_string());

    let state = provider.resolve_token_input_state(&settings).unwrap();

    assert!(state.has_token);
    assert_eq!(state.edit_mode, TokenEditMode::EditStored);
    assert_eq!(state.source_i18n_key, Some("cline_pass.source.config_file"));
    assert!(state.masked.is_some());
}

#[test]
fn usage_request_contract_uses_official_endpoint_and_bearer_auth() {
    assert_eq!(
        USAGE_URL,
        "https://api.cline.bot/api/v1/users/me/plan/usage-limits"
    );
    assert_eq!(
        cline_pass_auth_header("cline-key"),
        "Authorization: Bearer cline-key"
    );
}

#[test]
fn explicit_tokens_short_circuit_local_cline_credentials() {
    let configured = resolve_token_from_inputs(Some("configured-token"), Some("env-token"), || {
        panic!("local Cline credentials must not be read")
    })
    .unwrap();
    assert_eq!(configured.token.as_deref(), Some("configured-token"));
    assert_eq!(configured.source, ClineTokenSource::ConfigFile);

    let env = resolve_token_from_inputs(None, Some("env-token"), || {
        panic!("local Cline credentials must not be read")
    })
    .unwrap();
    assert_eq!(env.token.as_deref(), Some("env-token"));
    assert_eq!(env.source, ClineTokenSource::EnvVar);
}

#[test]
fn settings_path_follows_cline_environment_precedence() {
    assert_eq!(
        settings_path_from_sources(
            Some("/override/providers.json"),
            Some("/cline-data"),
            Some("/cline-root"),
            Some(Path::new("/home/test")),
        ),
        Some(PathBuf::from("/override/providers.json"))
    );
    assert_eq!(
        settings_path_from_sources(
            None,
            Some("/cline-data"),
            Some("/cline-root"),
            Some(Path::new("/home/test")),
        ),
        Some(PathBuf::from("/cline-data/settings/providers.json"))
    );
    assert_eq!(
        settings_path_from_sources(
            None,
            None,
            Some("/cline-root"),
            Some(Path::new("/home/test")),
        ),
        Some(PathBuf::from("/cline-root/data/settings/providers.json"))
    );
    assert_eq!(
        settings_path_from_sources(None, None, None, Some(Path::new("/home/test"))),
        Some(PathBuf::from(
            "/home/test/.cline/data/settings/providers.json"
        ))
    );
}

#[test]
fn providers_json_reads_cline_settings_api_key() {
    let body = r#"{
        "version": 1,
        "providers": {
            "cline": {
                "settings": {
                    "provider": "cline",
                    "apiKey": "cline-api-key"
                },
                "updatedAt": "2026-01-01T00:00:00Z",
                "tokenSource": "manual"
            }
        }
    }"#;

    assert_eq!(
        parse_providers_json(body, NOW_MS).unwrap().as_deref(),
        Some("cline-api-key")
    );
}

#[test]
fn providers_json_reads_cline_auth_api_key() {
    let body = r#"{
        "version": 1,
        "providers": {
            "cline": {
                "settings": {
                    "provider": "cline",
                    "auth": { "apiKey": "nested-api-key" }
                },
                "updatedAt": "2026-01-01T00:00:00Z",
                "tokenSource": "manual"
            }
        }
    }"#;

    assert_eq!(
        parse_providers_json(body, NOW_MS).unwrap().as_deref(),
        Some("nested-api-key")
    );
}

#[test]
fn providers_json_reads_unexpired_cline_oauth_access_token() {
    let body = r#"{
        "version": 1,
        "providers": {
            "cline": {
                "settings": {
                    "provider": "cline",
                    "auth": {
                        "accessToken": "workos:oauth-live",
                        "refreshToken": "refresh",
                        "expiresAt": 1900000000000
                    }
                },
                "updatedAt": "2026-01-01T00:00:00Z",
                "tokenSource": "oauth"
            }
        }
    }"#;

    assert_eq!(
        parse_providers_json(body, NOW_MS).unwrap().as_deref(),
        Some("workos:oauth-live")
    );
}

#[test]
fn providers_json_rejects_expired_cline_oauth_access_token() {
    let body = r#"{
        "version": 1,
        "providers": {
            "cline": {
                "settings": {
                    "provider": "cline",
                    "auth": {
                        "accessToken": "workos:oauth-expired",
                        "refreshToken": "refresh",
                        "expiresAt": 1700000000000
                    }
                },
                "updatedAt": "2026-01-01T00:00:00Z",
                "tokenSource": "oauth"
            }
        }
    }"#;

    let err = parse_providers_json(body, NOW_MS).expect_err("must not reuse expired OAuth");
    assert!(matches!(err, ProviderError::SessionExpired { .. }));
}

#[test]
fn providers_json_prefers_api_key_over_expired_oauth() {
    let body = r#"{
        "version": 1,
        "providers": {
            "cline": {
                "settings": {
                    "provider": "cline",
                    "apiKey": "fallback-api-key",
                    "auth": {
                        "accessToken": "workos:expired",
                        "expiresAt": 1700000000000
                    }
                },
                "updatedAt": "2026-01-01T00:00:00Z",
                "tokenSource": "manual"
            }
        }
    }"#;

    assert_eq!(
        parse_providers_json(body, NOW_MS).unwrap().as_deref(),
        Some("fallback-api-key")
    );
}

#[test]
fn providers_json_uses_jwt_expiry_and_normalizes_workos_prefix() {
    let jwt = jwt_with_exp(1_900_000_000);
    let body = format!(
        r#"{{
            "version": 1,
            "providers": {{
                "cline": {{
                    "settings": {{
                        "provider": "cline",
                        "auth": {{ "accessToken": "{jwt}" }}
                    }}
                }}
            }}
        }}"#
    );

    assert_eq!(
        parse_providers_json(&body, NOW_MS).unwrap(),
        Some(format!("workos:{jwt}"))
    );
}

#[test]
fn providers_json_rejects_oauth_when_expiry_cannot_be_proven() {
    let body = r#"{
        "version": 1,
        "providers": {
            "cline": {
                "settings": {
                    "provider": "cline",
                    "auth": { "accessToken": "opaque-token-without-expiry" }
                }
            }
        }
    }"#;

    let err = parse_providers_json(body, NOW_MS).expect_err("unknown expiry must relogin");
    assert!(matches!(err, ProviderError::SessionExpired { .. }));
}

#[test]
fn providers_json_rejects_oauth_expiring_during_request_timeout() {
    let body = format!(
        r#"{{
            "version": 1,
            "providers": {{
                "cline": {{
                    "settings": {{
                        "provider": "cline",
                        "auth": {{
                            "accessToken": "workos:near-expiry",
                            "expiresAt": {}
                        }}
                    }}
                }}
            }}
        }}"#,
        NOW_MS + 10_000
    );

    let err = parse_providers_json(&body, NOW_MS).expect_err("request needs expiry buffer");
    assert!(matches!(err, ProviderError::SessionExpired { .. }));
}

#[test]
fn providers_json_falls_back_to_legacy_cline_pass_entry() {
    let body = r#"{
        "version": 1,
        "providers": {
            "cline-pass": {
                "settings": {
                    "provider": "cline-pass",
                    "apiKey": "legacy-api-key"
                }
            }
        }
    }"#;

    assert_eq!(
        parse_providers_json(body, NOW_MS).unwrap().as_deref(),
        Some("legacy-api-key")
    );
}

#[test]
fn usage_response_maps_five_hour_weekly_and_monthly_windows() {
    let body = r#"{
        "success": true,
        "data": {
            "limits": [
                {
                    "type": "five_hour",
                    "percentUsed": 12.5,
                    "resetsAt": "2026-08-09T10:00:00Z"
                },
                {
                    "type": "weekly",
                    "percentUsed": 31.0,
                    "resetsAt": "2026-08-10T00:00:00Z"
                },
                {
                    "type": "monthly",
                    "percentUsed": 18.0,
                    "resetsAt": "2026-09-01T00:00:00Z"
                }
            ]
        }
    }"#;

    let quotas = parse_usage_response(body).expect("valid ClinePass usage response");

    assert_eq!(quotas.len(), 3);
    assert_eq!(quotas[0].stable_key, "session");
    assert_eq!(quotas[0].used, 12.5);
    assert_eq!(quotas[0].limit, 100.0);
    assert_eq!(quotas[0].quota_type, QuotaType::Session);
    assert_eq!(quotas[0].label_spec, QuotaLabelSpec::Session);
    assert_eq!(
        quotas[0].detail_spec,
        Some(QuotaDetailSpec::ResetAt {
            epoch_secs: 1_786_269_600,
        })
    );

    assert_eq!(quotas[1].stable_key, "weekly");
    assert_eq!(quotas[1].used, 31.0);
    assert_eq!(quotas[1].quota_type, QuotaType::Weekly);
    assert_eq!(quotas[1].label_spec, QuotaLabelSpec::Weekly);

    assert_eq!(quotas[2].stable_key, "monthly");
    assert_eq!(quotas[2].used, 18.0);
    assert_eq!(quotas[2].quota_type, QuotaType::Monthly);
    assert_eq!(quotas[2].label_spec, QuotaLabelSpec::Monthly);
    assert_eq!(
        quotas[2].detail_spec,
        Some(QuotaDetailSpec::ResetAt {
            epoch_secs: 1_788_220_800,
        })
    );
}

#[test]
fn usage_response_ignores_unknown_limits_and_allows_missing_reset() {
    let body = r#"{
        "success": true,
        "data": {
            "limits": [
                {
                    "type": "experimental_pool",
                    "resetsAt": "2026-08-09T10:00:00Z"
                },
                {
                    "type": "weekly",
                    "percentUsed": 25.0,
                    "resetsAt": null
                }
            ]
        }
    }"#;

    let quotas = parse_usage_response(body).expect("known limit remains usable");

    assert_eq!(quotas.len(), 1);
    assert_eq!(quotas[0].quota_type, QuotaType::Weekly);
    assert_eq!(quotas[0].detail_spec, None);
}

#[test]
fn usage_response_orders_windows_by_semantics_instead_of_payload_position() {
    let body = r#"{
        "success": true,
        "data": {
            "limits": [
                { "type": "monthly", "percentUsed": 18.0 },
                { "type": "five_hour", "percentUsed": 12.5 },
                { "type": "weekly", "percentUsed": 31.0 }
            ]
        }
    }"#;

    let quotas = parse_usage_response(body).expect("valid shuffled limits");

    assert_eq!(
        quotas
            .iter()
            .map(|quota| quota.quota_type.clone())
            .collect::<Vec<_>>(),
        vec![QuotaType::Session, QuotaType::Weekly, QuotaType::Monthly,]
    );
}

#[test]
fn known_limit_without_percent_used_is_rejected() {
    let body = r#"{
        "success": true,
        "data": {
            "limits": [{ "type": "monthly", "resetsAt": null }]
        }
    }"#;

    let err = parse_usage_response(body).expect_err("known windows require percentUsed");
    assert!(matches!(err, ProviderError::ParseFailed { .. }));
}
