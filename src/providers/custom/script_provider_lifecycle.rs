//! Script custom provider lifecycle operations.
//!
//! 本模块只持有脚本 provider 的身份、默认模板、文件名、编辑态加载、保存和删除语义。
//! 低层文件替换与回滚由 `file_ops.rs` 负责。

use crate::models::{
    script_provider_slug, ProviderId, ScriptProviderConfig, ScriptProviderEditData,
    DEFAULT_SCRIPT_TIMEOUT_MS,
};
use crate::providers::custom::file_ops;
use crate::providers::custom::generator;
use crate::providers::custom::lifecycle_error::{
    CustomProviderLifecycleError, CustomProviderLifecycleResult,
};
use crate::providers::custom::locator::{find_custom_provider_yaml_by_id, is_yaml_path};
use crate::providers::custom::schema::{CustomProviderDef, SourceDef};
use std::path::{Path, PathBuf};

pub(crate) fn generate_yaml_filename(config: &ScriptProviderConfig) -> String {
    yaml_filename_for_id(&config.provider_id)
        .unwrap_or_else(|| format!("script-{}.yaml", script_provider_slug(&config.display_name)))
}

pub(crate) fn generate_script_filename(config: &ScriptProviderConfig) -> String {
    script_filename_for_id(&config.provider_id)
        .unwrap_or_else(|| format!("script-{}.py", script_provider_slug(&config.display_name)))
}

pub(crate) fn default_template() -> &'static str {
    r#"import json
import os
import urllib.request

base_url = os.environ.get("CCSWITCH_BASE_URL", "https://example.com").rstrip("/")
api_key = os.environ.get("CCSWITCH_API_KEY", "")

request = urllib.request.Request(
    f"{base_url}/v1/usage",
    headers={"Authorization": f"Bearer {api_key}"},
)

with urllib.request.urlopen(request, timeout=15) as response:
    data = json.loads(response.read().decode("utf-8"))

quota = data.get("quota") or {}
remaining = data.get("remaining")
if remaining is None:
    remaining = quota.get("remaining")
if remaining is None:
    remaining = data.get("balance")

unit = data.get("unit") or quota.get("unit") or "USD"

print(json.dumps({
    "ok": data.get("is_active", data.get("isValid", True)),
    "remaining": remaining,
    "unit": unit,
}))
"#
}

pub(crate) fn read_config(provider_custom_id: &str) -> Option<ScriptProviderEditData> {
    read_config_in_dir(
        provider_custom_id,
        &crate::platform::paths::custom_providers_dir(),
    )
}

/// 保存脚本 provider 的 YAML 和 companion script。
pub(crate) fn save(
    config: &ScriptProviderConfig,
    yaml_filename: &str,
    script_filename: &str,
    allow_overwrite: bool,
) -> CustomProviderLifecycleResult<(PathBuf, PathBuf)> {
    let script_path = crate::platform::paths::custom_script_path(script_filename);
    let yaml_path = crate::platform::paths::custom_provider_path(yaml_filename);

    save_at_paths(config, &yaml_path, &script_path, allow_overwrite)
}

/// 删除 script provider 的 YAML 文件与 companion script。
pub(crate) fn delete_files(
    provider_id: &ProviderId,
) -> CustomProviderLifecycleResult<(PathBuf, CustomProviderLifecycleResult<PathBuf>)> {
    let custom_id = match provider_id {
        ProviderId::Custom(custom_id) => custom_id,
        _ => {
            return Err(CustomProviderLifecycleError::invalid_provider_id(
                "delete script provider",
                "custom",
                provider_id.to_string(),
            ))
        }
    };

    let (yaml_path, script_path) = find_paths(custom_id)?;
    delete_paths(&yaml_path, &script_path)
}

fn yaml_filename_for_id(custom_id: &str) -> Option<String> {
    let slug = custom_id.strip_suffix(":script")?;
    Some(format!("script-{}.yaml", slug))
}

fn script_filename_for_id(custom_id: &str) -> Option<String> {
    let slug = custom_id.strip_suffix(":script")?;
    Some(format!("script-{}.py", slug))
}

fn save_at_paths(
    config: &ScriptProviderConfig,
    yaml_path: &Path,
    script_path: &Path,
    allow_overwrite: bool,
) -> CustomProviderLifecycleResult<(PathBuf, PathBuf)> {
    if !allow_overwrite {
        file_ops::ensure_new_file(yaml_path).map_err(|err| {
            CustomProviderLifecycleError::file_operation("save script provider", err)
        })?;
        file_ops::ensure_new_file(script_path).map_err(|err| {
            CustomProviderLifecycleError::file_operation("save script provider", err)
        })?;
    }

    let committed_script_path = file_ops::versioned_script_path(script_path);
    let yaml_content = generator::generate_script_provider_yaml(config, &committed_script_path);
    file_ops::write_script_provider_files(
        &committed_script_path,
        yaml_path,
        &config.script,
        &yaml_content,
    )
    .map_err(|err| CustomProviderLifecycleError::file_operation("save script provider", err))?;

    // YAML 已经原子指向新版本；旧脚本清理失败只留下无害孤儿文件，不回滚生效配置。
    if script_path != committed_script_path && script_path.exists() {
        if let Err(error) = file_ops::delete_file_if_exists(script_path) {
            log::warn!(
                target: "providers::custom",
                "saved script provider but failed to remove old script version: {}",
                error
            );
        }
    }

    Ok((yaml_path.to_path_buf(), committed_script_path))
}

fn read_config_in_dir(
    provider_custom_id: &str,
    providers_dir: &Path,
) -> Option<ScriptProviderEditData> {
    let yaml = find_custom_provider_yaml_by_id(provider_custom_id, providers_dir)?;
    let step = yaml.def.plan.steps.first()?;
    let (interpreter, script_path, timeout_ms) = match &step.source {
        SourceDef::Cli {
            command,
            args,
            timeout_ms,
        } => (
            command.clone(),
            args.first()?.clone(),
            timeout_ms.unwrap_or(DEFAULT_SCRIPT_TIMEOUT_MS),
        ),
        _ => return None,
    };

    let script_path = PathBuf::from(script_path);
    let script = std::fs::read_to_string(&script_path).ok()?;
    let script_filename = script_path.file_name()?.to_str()?.to_string();

    Some(ScriptProviderEditData {
        display_name: yaml.def.metadata.display_name,
        provider_id: yaml.def.id,
        interpreter,
        timeout_ms,
        script,
        original_yaml_filename: yaml.filename,
        original_script_filename: script_filename,
    })
}

fn find_paths(custom_id: &str) -> CustomProviderLifecycleResult<(PathBuf, PathBuf)> {
    if !custom_id.ends_with(":script") {
        return Err(CustomProviderLifecycleError::invalid_provider_id(
            "delete script provider",
            "script",
            custom_id,
        ));
    }

    find_paths_in_dir(
        custom_id,
        &crate::platform::paths::custom_providers_dir(),
        &crate::platform::paths::custom_scripts_dir(),
    )
}

fn find_paths_in_dir(
    custom_id: &str,
    providers_dir: &Path,
    scripts_dir: &Path,
) -> CustomProviderLifecycleResult<(PathBuf, PathBuf)> {
    let entries = std::fs::read_dir(providers_dir).map_err(|e| {
        CustomProviderLifecycleError::file_operation(
            "delete script provider",
            format!(
                "failed to read providers dir {}: {}",
                providers_dir.display(),
                e
            ),
        )
    })?;
    for entry in entries.flatten() {
        let yaml_path = entry.path();
        if !is_yaml_path(&yaml_path) {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&yaml_path) else {
            continue;
        };
        let Ok(def) = serde_norway::from_str::<CustomProviderDef>(&content) else {
            continue;
        };
        if def.id != custom_id {
            continue;
        }
        let script_path = script_path_from_def(&def).ok_or_else(|| {
            CustomProviderLifecycleError::invalid_script_provider(
                "delete script provider",
                custom_id,
                "missing companion script path",
            )
        })?;
        let script_path = script_path_allowed_for_delete(script_path, scripts_dir)?;
        return Ok((yaml_path, script_path));
    }

    let fallback_yaml = yaml_filename_for_id(custom_id)
        .map(|filename| crate::platform::paths::custom_provider_path(&filename));
    Err(CustomProviderLifecycleError::yaml_not_found(
        "delete script provider",
        custom_id,
        fallback_yaml,
    ))
}

fn script_path_from_def(def: &CustomProviderDef) -> Option<PathBuf> {
    let step = def.plan.steps.first()?;
    match &step.source {
        SourceDef::Cli { args, .. } => args.first().map(PathBuf::from),
        _ => None,
    }
}

fn script_path_allowed_for_delete(
    path: PathBuf,
    scripts_dir: &Path,
) -> CustomProviderLifecycleResult<PathBuf> {
    let scripts_dir = std::fs::canonicalize(scripts_dir).map_err(|e| {
        CustomProviderLifecycleError::file_operation(
            "delete script provider",
            format!(
                "failed to resolve BananaTray scripts dir {}: {}",
                scripts_dir.display(),
                e
            ),
        )
    })?;
    let resolved_path = path_for_delete_boundary(&path)?;

    if resolved_path.starts_with(&scripts_dir) {
        Ok(path)
    } else {
        Err(CustomProviderLifecycleError::file_operation(
            "delete script provider",
            format!(
                "refusing to delete companion script outside BananaTray scripts dir: {}",
                path.display()
            ),
        ))
    }
}

fn path_for_delete_boundary(path: &Path) -> CustomProviderLifecycleResult<PathBuf> {
    if path.exists() {
        return std::fs::canonicalize(path).map_err(|e| {
            CustomProviderLifecycleError::file_operation(
                "delete script provider",
                format!(
                    "failed to resolve companion script path {}: {}",
                    path.display(),
                    e
                ),
            )
        });
    }

    let parent = path.parent().ok_or_else(|| {
        CustomProviderLifecycleError::file_operation(
            "delete script provider",
            format!(
                "failed to resolve companion script parent for {}",
                path.display()
            ),
        )
    })?;
    let parent = std::fs::canonicalize(parent).map_err(|e| {
        CustomProviderLifecycleError::file_operation(
            "delete script provider",
            format!(
                "failed to resolve companion script parent {}: {}",
                parent.display(),
                e
            ),
        )
    })?;
    let filename = path.file_name().ok_or_else(|| {
        CustomProviderLifecycleError::file_operation(
            "delete script provider",
            format!(
                "failed to resolve companion script filename for {}",
                path.display()
            ),
        )
    })?;
    Ok(parent.join(filename))
}

fn delete_paths(
    yaml_path: &Path,
    script_path: &Path,
) -> CustomProviderLifecycleResult<(PathBuf, CustomProviderLifecycleResult<PathBuf>)> {
    let deleted_yaml = file_ops::delete_yaml_file(yaml_path).map_err(|err| {
        CustomProviderLifecycleError::file_operation("delete script provider", err)
    })?;
    let deleted_script = file_ops::delete_file_if_exists(script_path)
        .map_err(|err| CustomProviderLifecycleError::file_operation("delete script provider", err));
    Ok((deleted_yaml, deleted_script))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_filenames_follow_provider_id_suffix() {
        let config = ScriptProviderConfig {
            display_name: "Anything".to_string(),
            provider_id: "relay-node:script".to_string(),
            interpreter: "python3".to_string(),
            timeout_ms: 12_000,
            script: "print(1)".to_string(),
        };

        assert_eq!(generate_yaml_filename(&config), "script-relay-node.yaml");
        assert_eq!(generate_script_filename(&config), "script-relay-node.py");
    }

    #[test]
    fn read_config_in_dir_reads_companion_script_and_actual_filenames() {
        let dir = tempfile::tempdir().unwrap();
        let providers_dir = dir.path().join("providers");
        let scripts_dir = dir.path().join("scripts");
        std::fs::create_dir_all(&providers_dir).unwrap();
        std::fs::create_dir_all(&scripts_dir).unwrap();
        let script_path = scripts_dir.join("renamed-script.py");
        std::fs::write(&script_path, "print('hello')").unwrap();
        let yaml_path = providers_dir.join("renamed-provider.yml");
        std::fs::write(&yaml_path, make_script_yaml("relay:script", &script_path)).unwrap();

        let edit = read_config_in_dir("relay:script", &providers_dir).unwrap();

        assert_eq!(edit.display_name, "Script");
        assert_eq!(edit.provider_id, "relay:script");
        assert_eq!(edit.interpreter, "python3");
        assert_eq!(edit.timeout_ms, DEFAULT_SCRIPT_TIMEOUT_MS);
        assert_eq!(edit.script, "print('hello')");
        assert_eq!(edit.original_yaml_filename, "renamed-provider.yml");
        assert_eq!(edit.original_script_filename, "renamed-script.py");
    }

    fn make_script_config(script: &str) -> ScriptProviderConfig {
        ScriptProviderConfig {
            display_name: "Script".to_string(),
            provider_id: "script:script".to_string(),
            interpreter: "python3".to_string(),
            timeout_ms: 12_000,
            script: script.to_string(),
        }
    }

    fn make_script_yaml(id: &str, script_path: &Path) -> String {
        format!(
            r#"id: "{id}"
schema_version: 2
metadata:
  display_name: "Script"
  brand_name: "Custom Script"
plan:
  steps:
    - name: "script"
      source:
        type: cli
        command: "python3"
        args:
          - "{}"
      parser:
        format: json
        quotas:
          - label: "Balance"
            remaining: "remaining"
"#,
            script_path.display()
        )
    }

    #[test]
    fn save_at_paths_writes_yaml_and_script() {
        let dir = tempfile::tempdir().unwrap();
        let yaml_path = dir.path().join("providers").join("script-test.yaml");
        let script_path = dir.path().join("scripts").join("script-test.py");

        let (saved_yaml, saved_script) = save_at_paths(
            &make_script_config("print(1)"),
            &yaml_path,
            &script_path,
            false,
        )
        .unwrap();

        assert_eq!(saved_yaml, yaml_path);
        assert_ne!(saved_script, script_path);
        assert_eq!(
            saved_script.parent(),
            script_path.parent(),
            "versioned script stays in the configured scripts directory"
        );
        assert_eq!(std::fs::read_to_string(&saved_script).unwrap(), "print(1)");
        let yaml = std::fs::read_to_string(&yaml_path).unwrap();
        assert!(yaml.contains("timeout_ms: 12000"));
        let def: crate::providers::custom::schema::CustomProviderDef =
            serde_norway::from_str(&yaml).unwrap();
        assert_eq!(def.id, "script:script");
    }

    #[test]
    fn save_at_paths_escapes_yaml_special_chars_and_spaces() {
        let dir = tempfile::tempdir().unwrap();
        let yaml_path = dir.path().join("providers").join("script test.yaml");
        let script_path = dir.path().join("scripts").join("script test.py");
        let config = ScriptProviderConfig {
            display_name: r#"Script "Quoted""#.to_string(),
            provider_id: "script-quoted:script".to_string(),
            interpreter: "python3".to_string(),
            timeout_ms: 12_000,
            script: "print(1)".to_string(),
        };

        let (_, saved_script) = save_at_paths(&config, &yaml_path, &script_path, false).unwrap();

        let yaml = std::fs::read_to_string(&yaml_path).unwrap();
        assert!(yaml.contains(r#"display_name: "Script \"Quoted\"""#));
        let def: crate::providers::custom::schema::CustomProviderDef =
            serde_norway::from_str(&yaml).unwrap();
        assert_eq!(def.metadata.display_name, r#"Script "Quoted""#);
        let SourceDef::Cli { args, .. } = &def.plan.steps[0].source else {
            panic!("expected cli source");
        };
        assert_eq!(
            args.first().map(String::as_str),
            Some(saved_script.to_str().unwrap())
        );
    }

    #[test]
    fn save_at_paths_refuses_create_over_existing_files() {
        let dir = tempfile::tempdir().unwrap();
        let yaml_path = dir.path().join("providers").join("script-test.yaml");
        let script_path = dir.path().join("scripts").join("script-test.py");
        std::fs::create_dir_all(yaml_path.parent().unwrap()).unwrap();
        std::fs::write(&yaml_path, "old yaml").unwrap();

        let err = save_at_paths(
            &make_script_config("print(1)"),
            &yaml_path,
            &script_path,
            false,
        )
        .unwrap_err();

        assert!(err.to_string().contains("refusing to overwrite"));
        assert_eq!(std::fs::read_to_string(&yaml_path).unwrap(), "old yaml");
    }

    #[test]
    fn delete_paths_reports_script_delete_failure_after_yaml_delete() {
        let dir = tempfile::tempdir().unwrap();
        let yaml_path = dir.path().join("script.yaml");
        let script_path = dir.path().join("script-dir");
        std::fs::write(&yaml_path, "id: script:script\n").unwrap();
        std::fs::create_dir(&script_path).unwrap();

        let (deleted_yaml, script_result) = delete_paths(&yaml_path, &script_path).unwrap();

        assert_eq!(deleted_yaml, yaml_path);
        assert!(!yaml_path.exists());
        assert!(script_result.is_err());
    }

    #[test]
    fn find_paths_uses_actual_yml_and_script_path() {
        let dir = tempfile::tempdir().unwrap();
        let providers_dir = dir.path().join("providers");
        let scripts_dir = dir.path().join("scripts");
        std::fs::create_dir_all(&providers_dir).unwrap();
        std::fs::create_dir_all(&scripts_dir).unwrap();
        let yaml_path = providers_dir.join("renamed-provider.yml");
        let script_path = scripts_dir.join("renamed-script.py");
        std::fs::write(&yaml_path, make_script_yaml("custom:script", &script_path)).unwrap();

        let (found_yaml, found_script) =
            find_paths_in_dir("custom:script", &providers_dir, &scripts_dir).unwrap();

        assert_eq!(found_yaml, yaml_path);
        assert_eq!(found_script, script_path);
    }

    #[test]
    fn find_paths_refuses_external_script_delete() {
        let dir = tempfile::tempdir().unwrap();
        let providers_dir = dir.path().join("providers");
        let scripts_dir = dir.path().join("scripts");
        let external_dir = dir.path().join("external");
        std::fs::create_dir_all(&providers_dir).unwrap();
        std::fs::create_dir_all(&scripts_dir).unwrap();
        std::fs::create_dir_all(&external_dir).unwrap();
        let yaml_path = providers_dir.join("provider.yaml");
        let script_path = external_dir.join("script.py");
        std::fs::write(&yaml_path, make_script_yaml("custom:script", &script_path)).unwrap();

        let err = find_paths_in_dir("custom:script", &providers_dir, &scripts_dir).unwrap_err();

        assert!(err.to_string().contains("outside BananaTray scripts dir"));
    }

    #[test]
    fn find_paths_refuses_parent_escape_from_scripts_dir() {
        let dir = tempfile::tempdir().unwrap();
        let providers_dir = dir.path().join("providers");
        let scripts_dir = dir.path().join("scripts");
        std::fs::create_dir_all(&providers_dir).unwrap();
        std::fs::create_dir_all(&scripts_dir).unwrap();
        let yaml_path = providers_dir.join("provider.yaml");
        let script_path = scripts_dir.join("..").join("escaped.py");
        std::fs::write(&yaml_path, make_script_yaml("custom:script", &script_path)).unwrap();

        let err = find_paths_in_dir("custom:script", &providers_dir, &scripts_dir).unwrap_err();

        assert!(err.to_string().contains("outside BananaTray scripts dir"));
    }
}
