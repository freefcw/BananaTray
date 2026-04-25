use anyhow::Result;

use crate::providers::common::cli;
use crate::providers::ProviderError;

use super::extractor;
use super::json_file::read_json_file;
use super::schema::AvailabilityDef;
use super::url::expand_tilde;

pub(super) fn check(def: &AvailabilityDef) -> Result<()> {
    match def {
        AvailabilityDef::CliExists { value } => check_cli_exists(value),
        AvailabilityDef::EnvVar { value } => check_env_var(value),
        AvailabilityDef::FileExists { value } => check_file_exists(value),
        AvailabilityDef::FileJsonMatch {
            path,
            json_path,
            expected,
        } => check_file_json_match(path, json_path, expected),
        AvailabilityDef::DirContains { path, prefix } => check_dir_contains(path, prefix),
        AvailabilityDef::Always => Ok(()),
    }
}

fn check_cli_exists(binary: &str) -> Result<()> {
    if cli::command_exists(binary) {
        Ok(())
    } else {
        Err(ProviderError::cli_not_found(binary).into())
    }
}

fn check_env_var(var: &str) -> Result<()> {
    if std::env::var(var).ok().filter(|v| !v.is_empty()).is_some() {
        Ok(())
    } else {
        Err(ProviderError::config_missing(var).into())
    }
}

fn check_file_json_match(path: &str, json_path: &str, expected: &str) -> Result<()> {
    let json = read_json_file(path)?;
    let actual = extractor::json_navigate(&json, json_path)
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if actual == expected {
        Ok(())
    } else {
        Err(ProviderError::config_missing(&format!(
            "{}:{} (expected '{}', got '{}')",
            path, json_path, expected, actual
        ))
        .into())
    }
}

fn check_dir_contains(path: &str, prefix: &str) -> Result<()> {
    let expanded = expand_tilde(path);
    if let Ok(entries) = std::fs::read_dir(&expanded) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.starts_with(prefix) {
                    return Ok(());
                }
            }
        }
    }
    Err(
        ProviderError::unavailable(&format!("no entry with prefix '{}' in {}", prefix, path))
            .into(),
    )
}

fn check_file_exists(path: &str) -> Result<()> {
    let expanded = expand_tilde(path);
    if std::path::Path::new(&expanded).exists() {
        Ok(())
    } else {
        Err(ProviderError::unavailable(&format!("file not found: {}", path)).into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check_sync(def: &AvailabilityDef) -> bool {
        check(def).is_ok()
    }

    #[test]
    fn test_check_env_var_set() {
        std::env::set_var("TEST_CUSTOM_AVAIL", "value");
        assert!(check_env_var("TEST_CUSTOM_AVAIL").is_ok());
        std::env::remove_var("TEST_CUSTOM_AVAIL");
    }

    #[test]
    fn test_check_env_var_missing() {
        std::env::remove_var("NONEXISTENT_CUSTOM_99");
        assert!(check_env_var("NONEXISTENT_CUSTOM_99").is_err());
    }

    #[test]
    fn test_check_file_exists_missing() {
        assert!(check_file_exists("/nonexistent/path/12345").is_err());
    }

    #[test]
    fn test_availability_always_is_ok() {
        assert!(check_sync(&AvailabilityDef::Always));
    }

    #[test]
    fn test_check_file_exists_existing_file() {
        assert!(check_file_exists("/etc/hosts").is_ok());
    }

    #[test]
    fn test_check_file_exists_tilde_expansion() {
        let home = dirs::home_dir().expect("should have home dir");
        let home_str = home.to_string_lossy();
        assert!(check_file_exists(&format!("{}", home_str)).is_ok());
    }

    #[test]
    fn test_check_file_exists_returns_unavailable_error() {
        let err = check_file_exists("/nonexistent/path/12345").unwrap_err();
        let provider_err = err.downcast_ref::<ProviderError>().unwrap();
        assert!(matches!(provider_err, ProviderError::Unavailable { .. }));
    }

    #[test]
    fn test_file_json_match_success() {
        let dir = tempfile::tempdir().unwrap();
        let json_path = dir.path().join("settings.json");
        std::fs::write(
            &json_path,
            r#"{"security":{"auth":{"selectedType":"vertex-ai"}}}"#,
        )
        .unwrap();
        let result = check_file_json_match(
            json_path.to_str().unwrap(),
            "security.auth.selectedType",
            "vertex-ai",
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_file_json_match_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let json_path = dir.path().join("settings.json");
        std::fs::write(
            &json_path,
            r#"{"security":{"auth":{"selectedType":"gemini"}}}"#,
        )
        .unwrap();
        let result = check_file_json_match(
            json_path.to_str().unwrap(),
            "security.auth.selectedType",
            "vertex-ai",
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("expected 'vertex-ai'"));
        assert!(err.to_string().contains("got 'gemini'"));
    }

    #[test]
    fn test_file_json_match_file_not_found() {
        let result = check_file_json_match("/nonexistent/path/settings.json", "some.path", "value");
        assert!(result.is_err());
    }

    #[test]
    fn test_file_json_match_invalid_json() {
        let dir = tempfile::tempdir().unwrap();
        let json_path = dir.path().join("bad.json");
        std::fs::write(&json_path, "not json").unwrap();
        let result = check_file_json_match(json_path.to_str().unwrap(), "some.path", "value");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("invalid JSON"));
    }

    #[test]
    fn test_dir_contains_success() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("kilocode.kilo-code-1.0.0")).unwrap();
        let result = check_dir_contains(dir.path().to_str().unwrap(), "kilocode.kilo-code");
        assert!(result.is_ok());
    }

    #[test]
    fn test_dir_contains_no_match() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("other-extension-1.0")).unwrap();
        let result = check_dir_contains(dir.path().to_str().unwrap(), "kilocode.kilo-code");
        assert!(result.is_err());
    }

    #[test]
    fn test_dir_contains_nonexistent_dir() {
        let result = check_dir_contains("/nonexistent/path/12345", "some-prefix");
        assert!(result.is_err());
    }

    #[test]
    fn test_availability_sync_file_json_match() {
        let dir = tempfile::tempdir().unwrap();
        let json_path = dir.path().join("config.json");
        std::fs::write(&json_path, r#"{"mode":"enabled"}"#).unwrap();

        let def = AvailabilityDef::FileJsonMatch {
            path: json_path.to_str().unwrap().to_string(),
            json_path: "mode".to_string(),
            expected: "enabled".to_string(),
        };
        assert!(check_sync(&def));

        let def_mismatch = AvailabilityDef::FileJsonMatch {
            path: json_path.to_str().unwrap().to_string(),
            json_path: "mode".to_string(),
            expected: "disabled".to_string(),
        };
        assert!(!check_sync(&def_mismatch));
    }

    #[test]
    fn test_availability_sync_dir_contains() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("my-extension-v1")).unwrap();

        let def = AvailabilityDef::DirContains {
            path: dir.path().to_str().unwrap().to_string(),
            prefix: "my-extension".to_string(),
        };
        assert!(check_sync(&def));

        let def_miss = AvailabilityDef::DirContains {
            path: dir.path().to_str().unwrap().to_string(),
            prefix: "other-extension".to_string(),
        };
        assert!(!check_sync(&def_miss));
    }
}
