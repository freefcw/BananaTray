//! NewAPI Provider YAML 文件的磁盘 I/O 操作。
//!
//! 封装 YAML 生成、目录创建、文件写入 / 删除等 I/O 步骤，
//! 供 `runtime/effects/newapi.rs` 中的 NewAPI effect handlers 调用。

use crate::models::{NewApiConfig, ProviderId, ScriptProviderConfig};
use crate::providers::custom::generator;
use crate::providers::custom::locator::{find_custom_provider_yaml_by_id, is_yaml_path};
use crate::providers::custom::schema::{CustomProviderDef, SourceDef};
use std::path::{Path, PathBuf};

/// 将 NewAPI 配置写入磁盘 YAML 文件。
///
/// 步骤：
/// 1. 生成 YAML 内容（`generator::generate_newapi_yaml`）
/// 2. 计算文件路径（`custom_provider_path`）
/// 3. 确保目录存在（`create_dir_all`）
/// 4. 写入文件（`fs::write`）
///
/// 成功返回文件路径，失败返回错误描述。
pub fn save_newapi_yaml(config: &NewApiConfig, filename: &str) -> Result<PathBuf, String> {
    let yaml_content = generator::generate_newapi_yaml(config);
    let path = crate::platform::paths::custom_provider_path(filename);

    let providers_dir = path
        .parent()
        .ok_or_else(|| format!("failed to resolve providers dir for {}", filename))?;

    std::fs::create_dir_all(providers_dir)
        .map_err(|e| format!("failed to create providers dir: {}", e))?;

    std::fs::write(&path, &yaml_content)
        .map_err(|e| format!("failed to write YAML to {}: {}", path.display(), e))?;

    Ok(path)
}

pub fn save_script_provider(
    config: &ScriptProviderConfig,
    yaml_filename: &str,
    script_filename: &str,
    allow_overwrite: bool,
) -> Result<(PathBuf, PathBuf), String> {
    let script_path = crate::platform::paths::custom_script_path(script_filename);
    let yaml_path = crate::platform::paths::custom_provider_path(yaml_filename);

    let scripts_dir = script_path
        .parent()
        .ok_or_else(|| format!("failed to resolve scripts dir for {}", script_filename))?;
    std::fs::create_dir_all(scripts_dir)
        .map_err(|e| format!("failed to create scripts dir: {}", e))?;
    let providers_dir = yaml_path
        .parent()
        .ok_or_else(|| format!("failed to resolve providers dir for {}", yaml_filename))?;
    std::fs::create_dir_all(providers_dir)
        .map_err(|e| format!("failed to create providers dir: {}", e))?;

    if !allow_overwrite {
        ensure_new_file(&yaml_path)?;
        ensure_new_file(&script_path)?;
    }

    let yaml_content = generator::generate_script_provider_yaml(config, &script_path);
    let script_tmp = temp_sibling_path(&script_path);
    let yaml_tmp = temp_sibling_path(&yaml_path);

    if let Err(err) = write_script_provider_files(
        &script_tmp,
        &yaml_tmp,
        &script_path,
        &yaml_path,
        &config.script,
        &yaml_content,
    ) {
        let _ = std::fs::remove_file(&script_tmp);
        let _ = std::fs::remove_file(&yaml_tmp);
        return Err(err);
    }

    Ok((yaml_path, script_path))
}

#[cfg(test)]
fn save_script_provider_at_paths(
    config: &ScriptProviderConfig,
    yaml_path: &Path,
    script_path: &Path,
    allow_overwrite: bool,
) -> Result<(PathBuf, PathBuf), String> {
    let scripts_dir = script_path.parent().ok_or_else(|| {
        format!(
            "failed to resolve scripts dir for {}",
            script_path.display()
        )
    })?;
    std::fs::create_dir_all(scripts_dir)
        .map_err(|e| format!("failed to create scripts dir: {}", e))?;
    let providers_dir = yaml_path.parent().ok_or_else(|| {
        format!(
            "failed to resolve providers dir for {}",
            yaml_path.display()
        )
    })?;
    std::fs::create_dir_all(providers_dir)
        .map_err(|e| format!("failed to create providers dir: {}", e))?;

    if !allow_overwrite {
        ensure_new_file(yaml_path)?;
        ensure_new_file(script_path)?;
    }

    let yaml_content = generator::generate_script_provider_yaml(config, script_path);
    let script_tmp = temp_sibling_path(script_path);
    let yaml_tmp = temp_sibling_path(yaml_path);
    if let Err(err) = write_script_provider_files(
        &script_tmp,
        &yaml_tmp,
        script_path,
        yaml_path,
        &config.script,
        &yaml_content,
    ) {
        let _ = std::fs::remove_file(&script_tmp);
        let _ = std::fs::remove_file(&yaml_tmp);
        return Err(err);
    }

    Ok((yaml_path.to_path_buf(), script_path.to_path_buf()))
}

fn write_script_provider_files(
    script_tmp: &Path,
    yaml_tmp: &Path,
    script_path: &Path,
    yaml_path: &Path,
    script_content: &str,
    yaml_content: &str,
) -> Result<(), String> {
    std::fs::write(script_tmp, script_content)
        .map_err(|e| format!("failed to write script to {}: {}", script_tmp.display(), e))?;
    std::fs::write(yaml_tmp, yaml_content)
        .map_err(|e| format!("failed to write YAML to {}: {}", yaml_tmp.display(), e))?;

    let script_backup = backup_existing_file(script_path)?;
    let yaml_backup = backup_existing_file(yaml_path)?;

    if let Err(err) = try_rename(script_tmp, script_path) {
        restore_backup(script_path, script_backup.as_deref());
        restore_backup(yaml_path, yaml_backup.as_deref());
        return Err(err);
    }

    if let Err(err) = try_rename(yaml_tmp, yaml_path) {
        restore_backup(script_path, script_backup.as_deref());
        restore_backup(yaml_path, yaml_backup.as_deref());
        return Err(err);
    }

    cleanup_backup(script_backup.as_deref());
    cleanup_backup(yaml_backup.as_deref());
    Ok(())
}

fn ensure_new_file(path: &Path) -> Result<(), String> {
    if path.exists() {
        Err(format!(
            "refusing to overwrite existing file {}",
            path.display()
        ))
    } else {
        Ok(())
    }
}

fn temp_sibling_path(path: &Path) -> PathBuf {
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("tmp");
    path.with_file_name(format!(
        ".{}.{}.tmp",
        filename,
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ))
}

fn backup_sibling_path(path: &Path) -> PathBuf {
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("backup");
    path.with_file_name(format!(
        ".{}.{}.bak",
        filename,
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ))
}

fn backup_existing_file(path: &Path) -> Result<Option<PathBuf>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let backup = backup_sibling_path(path);
    std::fs::rename(path, &backup).map_err(|e| {
        format!(
            "failed to back up {} to {}: {}",
            path.display(),
            backup.display(),
            e
        )
    })?;
    Ok(Some(backup))
}

fn try_rename(tmp: &Path, dest: &Path) -> Result<(), String> {
    std::fs::rename(tmp, dest).map_err(|e| {
        format!(
            "failed to replace {} from {}: {}",
            dest.display(),
            tmp.display(),
            e
        )
    })
}

fn restore_backup(path: &Path, backup: Option<&Path>) {
    if let Some(backup) = backup {
        let _ = std::fs::remove_file(path);
        let _ = std::fs::rename(backup, path);
    }
}

fn cleanup_backup(backup: Option<&Path>) {
    if let Some(backup) = backup {
        let _ = std::fs::remove_file(backup);
    }
}

/// 删除 NewAPI 配置对应的 YAML 文件。
///
/// 步骤：
/// 1. 校验 provider id 是 NewAPI custom provider
/// 2. 计算文件路径
/// 3. 删除 YAML 文件
///
/// 成功返回被删除的文件路径，失败返回错误描述。
pub fn delete_newapi_yaml(provider_id: &ProviderId) -> Result<PathBuf, String> {
    let custom_id = match provider_id {
        ProviderId::Custom(custom_id) => custom_id,
        _ => {
            return Err(format!(
                "NewApiEffect::DeleteProvider: not a custom provider id: {provider_id}"
            ))
        }
    };

    let yaml_path = find_newapi_yaml_path(custom_id)?;
    delete_yaml_file(&yaml_path)
}

pub fn delete_script_provider_files(
    provider_id: &ProviderId,
) -> Result<(PathBuf, Result<PathBuf, String>), String> {
    let custom_id = match provider_id {
        ProviderId::Custom(custom_id) => custom_id,
        _ => {
            return Err(format!(
                "ScriptProviderEffect::DeleteProvider: not a custom provider id: {provider_id}"
            ))
        }
    };

    let (yaml_path, script_path) = find_script_provider_paths(custom_id)?;
    delete_script_provider_paths(&yaml_path, &script_path)
}

fn find_script_provider_paths(custom_id: &str) -> Result<(PathBuf, PathBuf), String> {
    if !custom_id.ends_with(":script") {
        return Err(format!(
            "ScriptProviderEffect::DeleteProvider: not a script provider id: {custom_id}"
        ));
    }

    find_script_provider_paths_in_dir(
        custom_id,
        &crate::platform::paths::custom_providers_dir(),
        &crate::platform::paths::custom_scripts_dir(),
    )
}

fn find_newapi_yaml_path(custom_id: &str) -> Result<PathBuf, String> {
    if !custom_id.ends_with(":newapi") {
        return Err(format!(
            "NewApiEffect::DeleteProvider: not a newapi provider id: {custom_id}"
        ));
    }

    let providers_dir = crate::platform::paths::custom_providers_dir();
    if let Ok(path) = find_newapi_yaml_path_in_dir(custom_id, &providers_dir) {
        return Ok(path);
    }

    let fallback_yaml = generator::filename_for_id(custom_id)
        .map(|filename| crate::platform::paths::custom_provider_path(&filename));
    match fallback_yaml {
        Some(path) => Err(format!(
            "NewApiEffect::DeleteProvider: NewAPI provider YAML not found for {} (expected {} or matching .yml)",
            custom_id,
            path.display()
        )),
        None => Err(format!(
            "NewApiEffect::DeleteProvider: not a newapi provider id: {custom_id}"
        )),
    }
}

fn find_newapi_yaml_path_in_dir(custom_id: &str, providers_dir: &Path) -> Result<PathBuf, String> {
    find_custom_provider_yaml_by_id(custom_id, providers_dir)
        .map(|yaml| yaml.path)
        .ok_or_else(|| {
            format!("NewApiEffect::DeleteProvider: NewAPI provider YAML not found for {custom_id}")
        })
}

fn find_script_provider_paths_in_dir(
    custom_id: &str,
    providers_dir: &Path,
    scripts_dir: &Path,
) -> Result<(PathBuf, PathBuf), String> {
    let entries = std::fs::read_dir(providers_dir).map_err(|e| {
        format!(
            "failed to read providers dir {}: {}",
            providers_dir.display(),
            e
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
            format!(
                "ScriptProviderEffect::DeleteProvider: script provider {} has no script path",
                custom_id
            )
        })?;
        let script_path = script_path_allowed_for_delete(script_path, scripts_dir)?;
        return Ok((yaml_path, script_path));
    }

    let fallback_yaml = generator::script_yaml_filename_for_id(custom_id)
        .map(|filename| crate::platform::paths::custom_provider_path(&filename));
    match fallback_yaml {
        Some(path) => Err(format!(
            "ScriptProviderEffect::DeleteProvider: script provider YAML not found for {} (expected {} or matching .yml)",
            custom_id,
            path.display()
        )),
        None => Err(format!(
            "ScriptProviderEffect::DeleteProvider: not a script provider id: {custom_id}"
        )),
    }
}

fn script_path_from_def(def: &CustomProviderDef) -> Option<PathBuf> {
    let step = def.plan.steps.first()?;
    match &step.source {
        SourceDef::Cli { args, .. } => args.first().map(PathBuf::from),
        _ => None,
    }
}

fn script_path_allowed_for_delete(path: PathBuf, scripts_dir: &Path) -> Result<PathBuf, String> {
    let scripts_dir = std::fs::canonicalize(scripts_dir).map_err(|e| {
        format!(
            "failed to resolve BananaTray scripts dir {}: {}",
            scripts_dir.display(),
            e
        )
    })?;
    let resolved_path = path_for_delete_boundary(&path)?;

    if resolved_path.starts_with(&scripts_dir) {
        Ok(path)
    } else {
        Err(format!(
            "refusing to delete companion script outside BananaTray scripts dir: {}",
            path.display()
        ))
    }
}

fn path_for_delete_boundary(path: &Path) -> Result<PathBuf, String> {
    if path.exists() {
        return std::fs::canonicalize(path).map_err(|e| {
            format!(
                "failed to resolve companion script path {}: {}",
                path.display(),
                e
            )
        });
    }

    let parent = path.parent().ok_or_else(|| {
        format!(
            "failed to resolve companion script parent for {}",
            path.display()
        )
    })?;
    let parent = std::fs::canonicalize(parent).map_err(|e| {
        format!(
            "failed to resolve companion script parent {}: {}",
            parent.display(),
            e
        )
    })?;
    let filename = path.file_name().ok_or_else(|| {
        format!(
            "failed to resolve companion script filename for {}",
            path.display()
        )
    })?;
    Ok(parent.join(filename))
}

pub fn delete_script_provider_paths(
    yaml_path: &Path,
    script_path: &Path,
) -> Result<(PathBuf, Result<PathBuf, String>), String> {
    let deleted_yaml = delete_yaml_file(yaml_path)?;
    let deleted_script = delete_file_if_exists(script_path);
    Ok((deleted_yaml, deleted_script))
}

fn delete_yaml_file(path: &Path) -> Result<PathBuf, String> {
    std::fs::remove_file(path)
        .map(|()| path.to_path_buf())
        .map_err(|e| format!("failed to delete YAML {}: {}", path.display(), e))
}

fn delete_file_if_exists(path: &Path) -> Result<PathBuf, String> {
    if !path.exists() {
        return Ok(path.to_path_buf());
    }
    std::fs::remove_file(path)
        .map(|()| path.to_path_buf())
        .map_err(|e| format!("failed to delete file {}: {}", path.display(), e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ProviderKind, ScriptProviderConfig};

    #[test]
    fn delete_yaml_file_removes_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("relay.yaml");
        std::fs::write(&path, "id: test:newapi\n").unwrap();

        let deleted_path = delete_yaml_file(&path).unwrap();

        assert_eq!(deleted_path, path);
        assert!(!path.exists());
    }

    #[test]
    fn delete_newapi_yaml_rejects_builtin_provider() {
        let err = delete_newapi_yaml(&ProviderId::BuiltIn(ProviderKind::Claude)).unwrap_err();
        assert!(err.contains("not a custom provider id"));
    }

    #[test]
    fn delete_newapi_yaml_rejects_non_newapi_custom_provider() {
        let err = delete_newapi_yaml(&ProviderId::Custom("custom:cli".to_string())).unwrap_err();
        assert!(err.contains("not a newapi provider id"));
    }

    #[test]
    fn find_newapi_yaml_path_matches_yaml_id_not_filename() {
        let dir = tempfile::tempdir().unwrap();
        let providers_dir = dir.path().join("providers");
        std::fs::create_dir_all(&providers_dir).unwrap();
        let yaml_path = providers_dir.join("renamed-provider.yaml");
        std::fs::write(
            &yaml_path,
            r#"id: "my-api:newapi"
schema_version: 2
base_url: "https://example.com"
metadata:
  display_name: "Example"
  brand_name: "NewAPI Relay"
plan:
  steps:
    - name: "api"
      source:
        type: http
        method: get
        url: "/api/user/self"
      parser:
        format: json
        quotas:
          - label: "Balance"
            remaining: "data.quota"
"#,
        )
        .unwrap();

        let found = find_newapi_yaml_path_in_dir("my-api:newapi", &providers_dir).unwrap();
        assert_eq!(found, yaml_path);
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
    fn save_script_provider_at_paths_writes_yaml_and_script() {
        let dir = tempfile::tempdir().unwrap();
        let yaml_path = dir.path().join("providers").join("script-test.yaml");
        let script_path = dir.path().join("scripts").join("script-test.py");

        let (saved_yaml, saved_script) = save_script_provider_at_paths(
            &make_script_config("print(1)"),
            &yaml_path,
            &script_path,
            false,
        )
        .unwrap();

        assert_eq!(saved_yaml, yaml_path);
        assert_eq!(saved_script, script_path);
        assert_eq!(std::fs::read_to_string(&script_path).unwrap(), "print(1)");
        let yaml = std::fs::read_to_string(&yaml_path).unwrap();
        assert!(yaml.contains("timeout_ms: 12000"));
        let def: crate::providers::custom::schema::CustomProviderDef =
            serde_norway::from_str(&yaml).unwrap();
        assert_eq!(def.id, "script:script");
    }

    #[test]
    fn save_script_provider_at_paths_escapes_yaml_special_chars_and_spaces() {
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

        save_script_provider_at_paths(&config, &yaml_path, &script_path, false).unwrap();

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
            Some(script_path.to_str().unwrap())
        );
    }

    #[test]
    fn save_script_provider_at_paths_refuses_create_over_existing_files() {
        let dir = tempfile::tempdir().unwrap();
        let yaml_path = dir.path().join("providers").join("script-test.yaml");
        let script_path = dir.path().join("scripts").join("script-test.py");
        std::fs::create_dir_all(yaml_path.parent().unwrap()).unwrap();
        std::fs::write(&yaml_path, "old yaml").unwrap();

        let err = save_script_provider_at_paths(
            &make_script_config("print(1)"),
            &yaml_path,
            &script_path,
            false,
        )
        .unwrap_err();

        assert!(err.contains("refusing to overwrite"));
        assert_eq!(std::fs::read_to_string(&yaml_path).unwrap(), "old yaml");
    }

    #[test]
    fn save_script_provider_at_paths_leaves_old_files_when_yaml_temp_write_fails() {
        let dir = tempfile::tempdir().unwrap();
        let yaml_dir = dir.path().join("providers");
        let script_dir = dir.path().join("scripts");
        std::fs::create_dir_all(&yaml_dir).unwrap();
        std::fs::create_dir_all(&script_dir).unwrap();
        let yaml_path = yaml_dir.join("script-test.yaml");
        let script_path = script_dir.join("script-test.py");
        std::fs::write(&yaml_path, "old yaml").unwrap();
        std::fs::write(&script_path, "old script").unwrap();

        let yaml_blocker = yaml_dir.join(format!(
            ".script-test.yaml.{}.tmp",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::create_dir(&yaml_blocker).unwrap();
        let err = write_script_provider_files(
            &script_dir.join("script-test.py.tmp"),
            &yaml_blocker,
            &script_path,
            &yaml_path,
            "new script",
            "new yaml",
        )
        .unwrap_err();

        assert!(err.contains("failed to write YAML"));
        assert_eq!(std::fs::read_to_string(&yaml_path).unwrap(), "old yaml");
        assert_eq!(std::fs::read_to_string(&script_path).unwrap(), "old script");
    }

    #[test]
    fn restore_backup_restores_original_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("script.py");
        std::fs::write(&path, "old script").unwrap();
        let backup = backup_existing_file(&path).unwrap().unwrap();
        std::fs::write(&path, "new script").unwrap();

        restore_backup(&path, Some(&backup));

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "old script");
        assert!(!backup.exists());
    }

    #[test]
    fn write_script_provider_files_rolls_back_script_when_yaml_rename_fails() {
        let dir = tempfile::tempdir().unwrap();
        let script_dir = dir.path().join("scripts");
        std::fs::create_dir_all(&script_dir).unwrap();
        let script_path = script_dir.join("test.py");
        std::fs::write(&script_path, "old script").unwrap();

        let script_tmp = script_dir.join("test.py.tmp");
        let yaml_tmp = dir.path().join("test.yaml.tmp");
        // yaml_path 的父目录不存在 → rename 必然失败
        let yaml_path = dir.path().join("nonexistent_dir").join("test.yaml");

        let err = write_script_provider_files(
            &script_tmp,
            &yaml_tmp,
            &script_path,
            &yaml_path,
            "new script",
            "new yaml",
        )
        .unwrap_err();

        assert!(err.contains("failed to replace"));
        assert_eq!(std::fs::read_to_string(&script_path).unwrap(), "old script");
    }

    #[test]
    fn rollback_restores_both_files_after_partial_rename() {
        let dir = tempfile::tempdir().unwrap();
        let script_path = dir.path().join("script.py");
        let yaml_path = dir.path().join("config.yaml");
        std::fs::write(&script_path, "old script").unwrap();
        std::fs::write(&yaml_path, "old yaml").unwrap();

        // 模拟：备份两个文件，script rename 成功后 yaml rename 失败的回滚路径
        let script_backup = backup_existing_file(&script_path).unwrap();
        let yaml_backup = backup_existing_file(&yaml_path).unwrap();
        std::fs::write(&script_path, "new script").unwrap();

        restore_backup(&script_path, script_backup.as_deref());
        restore_backup(&yaml_path, yaml_backup.as_deref());

        assert_eq!(std::fs::read_to_string(&script_path).unwrap(), "old script");
        assert_eq!(std::fs::read_to_string(&yaml_path).unwrap(), "old yaml");
    }

    #[test]
    fn delete_script_provider_paths_reports_script_delete_failure_after_yaml_delete() {
        let dir = tempfile::tempdir().unwrap();
        let yaml_path = dir.path().join("script.yaml");
        let script_path = dir.path().join("script-dir");
        std::fs::write(&yaml_path, "id: script:script\n").unwrap();
        std::fs::create_dir(&script_path).unwrap();

        let (deleted_yaml, script_result) =
            delete_script_provider_paths(&yaml_path, &script_path).unwrap();

        assert_eq!(deleted_yaml, yaml_path);
        assert!(!yaml_path.exists());
        assert!(script_result.is_err());
    }

    #[test]
    fn find_script_provider_paths_uses_actual_yml_and_script_path() {
        let dir = tempfile::tempdir().unwrap();
        let providers_dir = dir.path().join("providers");
        let scripts_dir = dir.path().join("scripts");
        std::fs::create_dir_all(&providers_dir).unwrap();
        std::fs::create_dir_all(&scripts_dir).unwrap();
        let yaml_path = providers_dir.join("renamed-provider.yml");
        let script_path = scripts_dir.join("renamed-script.py");
        std::fs::write(&yaml_path, make_script_yaml("custom:script", &script_path)).unwrap();

        let (found_yaml, found_script) =
            find_script_provider_paths_in_dir("custom:script", &providers_dir, &scripts_dir)
                .unwrap();

        assert_eq!(found_yaml, yaml_path);
        assert_eq!(found_script, script_path);
    }

    #[test]
    fn find_script_provider_paths_refuses_external_script_delete() {
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

        let err = find_script_provider_paths_in_dir("custom:script", &providers_dir, &scripts_dir)
            .unwrap_err();

        assert!(err.contains("outside BananaTray scripts dir"));
    }

    #[test]
    fn find_script_provider_paths_refuses_parent_escape_from_scripts_dir() {
        let dir = tempfile::tempdir().unwrap();
        let providers_dir = dir.path().join("providers");
        let scripts_dir = dir.path().join("scripts");
        std::fs::create_dir_all(&providers_dir).unwrap();
        std::fs::create_dir_all(&scripts_dir).unwrap();
        let yaml_path = providers_dir.join("provider.yaml");
        let script_path = scripts_dir.join("..").join("escaped.py");
        std::fs::write(&yaml_path, make_script_yaml("custom:script", &script_path)).unwrap();

        let err = find_script_provider_paths_in_dir("custom:script", &providers_dir, &scripts_dir)
            .unwrap_err();

        assert!(err.contains("outside BananaTray scripts dir"));
    }
}
