//! Custom script provider pure data and ID helpers.
//!
//! The runtime still uses the existing custom-provider YAML `cli` source.
//! These types only describe the Settings UI wizard input and the generated
//! files around that YAML.

use serde_json::Value;

/// Script provider form submission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptProviderConfig {
    /// Display name shown in the provider list.
    pub display_name: String,
    /// Stable custom provider id, e.g. `ccswitch:script`.
    pub provider_id: String,
    /// Interpreter command, normally `python3`.
    pub interpreter: String,
    /// Script execution timeout. The UI displays seconds, and generated YAML
    /// stores milliseconds in `source.timeout_ms`.
    pub timeout_ms: u64,
    /// Python source code written to the scripts directory.
    pub script: String,
}

/// Data loaded from disk to refill the script editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptProviderEditData {
    pub display_name: String,
    pub provider_id: String,
    pub interpreter: String,
    pub timeout_ms: u64,
    pub script: String,
    pub original_yaml_filename: String,
    pub original_script_filename: String,
}

/// Preview data parsed from script stdout.
#[derive(Debug, Clone, PartialEq)]
pub struct ScriptProviderQuotaPreview {
    pub label: String,
    pub remaining: f64,
    pub used: Option<f64>,
    pub unit: String,
}

/// Last result of the Settings UI "Run Test" action.
#[derive(Debug, Clone, PartialEq)]
pub struct ScriptProviderTestResult {
    pub success: bool,
    pub message: String,
    pub stdout: String,
    pub stderr: String,
    pub preview: Option<ScriptProviderQuotaPreview>,
}

/// Default runtime shown by the script editor.
pub const DEFAULT_SCRIPT_INTERPRETER: &str = "python3";
pub const DEFAULT_SCRIPT_TIMEOUT_MS: u64 = 20_000;

/// Slugify a user-facing name for provider IDs and filenames.
pub fn script_provider_slug(name: &str) -> String {
    let slug = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .to_ascii_lowercase()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");

    if slug.is_empty() {
        "custom-script".to_string()
    } else {
        slug
    }
}

/// Build a script-provider id from an already validated slug.
pub fn script_provider_id_from_slug(slug: &str) -> String {
    format!("{slug}:script")
}

/// Compute the custom provider id for a script provider.
pub fn script_provider_id(name: &str) -> String {
    script_provider_id_from_slug(&script_provider_slug(name))
}

/// Compute the first available script-provider id for a user-facing name.
pub fn unique_script_provider_id(name: &str, mut is_occupied: impl FnMut(&str) -> bool) -> String {
    let base_slug = script_provider_slug(name);
    let mut suffix = 1;

    loop {
        let slug = if suffix == 1 {
            base_slug.clone()
        } else {
            format!("{base_slug}-{suffix}")
        };
        let id = script_provider_id_from_slug(&slug);
        if !is_occupied(&id) {
            return id;
        }
        suffix += 1;
    }
}

/// Parse stdout emitted by a script-provider script.
///
/// Stable MVP contract:
/// - stdout must be JSON
/// - `ok: false`, `isValid: false`, or `is_active: false` marks it invalid
/// - top-level `remaining` must be numeric or numeric string
/// - top-level `used` and `unit` are optional
pub fn parse_script_stdout(stdout: &str) -> Result<ScriptProviderQuotaPreview, String> {
    let json: Value =
        serde_json::from_str(stdout).map_err(|e| format!("stdout is not valid JSON: {e}"))?;

    if bool_field_is_false(&json, "ok")
        || bool_field_is_false(&json, "isValid")
        || bool_field_is_false(&json, "is_active")
    {
        return Err("script returned an inactive result".to_string());
    }

    let remaining = numeric_field(&json, "remaining")
        .ok_or_else(|| "stdout JSON must include numeric field 'remaining'".to_string())?;
    let used = numeric_field(&json, "used");
    let unit = string_field(&json, "unit").unwrap_or_else(|| "USD".to_string());
    let label = string_field(&json, "label").unwrap_or_else(|| "Balance".to_string());

    Ok(ScriptProviderQuotaPreview {
        label,
        remaining,
        used,
        unit,
    })
}

fn bool_field_is_false(json: &Value, key: &str) -> bool {
    json.get(key).and_then(Value::as_bool) == Some(false)
}

fn numeric_field(json: &Value, key: &str) -> Option<f64> {
    let value = json.get(key)?;
    let num = value
        .as_f64()
        .or_else(|| value.as_str().and_then(|s| s.parse::<f64>().ok()))?;
    num.is_finite().then_some(num)
}

fn string_field(json: &Value, key: &str) -> Option<String> {
    json.get(key)
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_provider_id_slugifies_name() {
        assert_eq!(script_provider_id("ccswitch"), "ccswitch:script");
        assert_eq!(script_provider_id("My API"), "my-api:script");
        assert_eq!(script_provider_id("月之暗面"), "custom-script:script");
    }

    #[test]
    fn script_provider_id_from_slug_keeps_suffix_contract() {
        assert_eq!(
            script_provider_id_from_slug("custom-script-2"),
            "custom-script-2:script"
        );
    }

    #[test]
    fn unique_script_provider_id_skips_occupied_ids() {
        let occupied = ["custom-script:script", "custom-script-2:script"];
        let id =
            unique_script_provider_id("月之暗面", |candidate| occupied.contains(&candidate));

        assert_eq!(id, "custom-script-3:script");
    }

    #[test]
    fn unique_script_provider_id_handles_many_collisions() {
        let occupied = [
            "my-api:script",
            "my-api-2:script",
            "my-api-3:script",
            "my-api-4:script",
        ];
        let id = unique_script_provider_id("My API", |candidate| occupied.contains(&candidate));

        assert_eq!(id, "my-api-5:script");
    }

    #[test]
    fn parse_script_stdout_reads_required_fields() {
        let preview = parse_script_stdout(r#"{"remaining":"12.5","used":3,"unit":"USD"}"#).unwrap();
        assert_eq!(preview.label, "Balance");
        assert_eq!(preview.remaining, 12.5);
        assert_eq!(preview.used, Some(3.0));
        assert_eq!(preview.unit, "USD");
    }

    #[test]
    fn parse_script_stdout_rejects_inactive_result() {
        let err = parse_script_stdout(r#"{"ok":false,"remaining":1}"#).unwrap_err();
        assert!(err.contains("inactive"));
    }

    #[test]
    fn parse_script_stdout_requires_remaining() {
        let err = parse_script_stdout(r#"{"balance":1}"#).unwrap_err();
        assert!(err.contains("remaining"));
    }

    #[test]
    fn parse_script_stdout_rejects_non_finite_numeric_strings() {
        for value in ["nan", "NaN", "inf", "Infinity", "-inf"] {
            let stdout = format!(r#"{{"remaining":"{value}"}}"#);
            let err = parse_script_stdout(&stdout).unwrap_err();
            assert!(err.contains("remaining"));
        }
    }

    #[test]
    fn parse_script_stdout_ignores_non_finite_optional_used() {
        let preview = parse_script_stdout(r#"{"remaining":12.5,"used":"inf"}"#).unwrap();

        assert_eq!(preview.remaining, 12.5);
        assert_eq!(preview.used, None);
    }
}
