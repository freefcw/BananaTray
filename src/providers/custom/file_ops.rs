//! Low-level custom provider file operations.
//!
//! This module owns atomic-ish sibling-temp replacement, backup/restore, and
//! delete primitives. Provider identity, YAML parsing, and user-facing lifecycle
//! decisions belong in `newapi_lifecycle.rs` / `script_provider_lifecycle.rs`.

use std::path::{Path, PathBuf};

pub(super) fn write_newapi_yaml(path: &Path, yaml_content: &str) -> Result<PathBuf, String> {
    let providers_dir = path
        .parent()
        .ok_or_else(|| format!("failed to resolve providers dir for {}", path.display()))?;
    std::fs::create_dir_all(providers_dir)
        .map_err(|e| format!("failed to create providers dir: {}", e))?;

    replace_file_atomically(path, yaml_content, "YAML")?;
    Ok(path.to_path_buf())
}

pub(super) fn write_script_provider_files(
    script_path: &Path,
    yaml_path: &Path,
    script_content: &str,
    yaml_content: &str,
) -> Result<(), String> {
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

    let script_tmp = temp_sibling_path(script_path);
    let yaml_tmp = temp_sibling_path(yaml_path);
    write_script_provider_files_with_temps(
        &script_tmp,
        &yaml_tmp,
        script_path,
        yaml_path,
        script_content,
        yaml_content,
    )
}

pub(super) fn ensure_new_file(path: &Path) -> Result<(), String> {
    if path.exists() {
        Err(format!(
            "refusing to overwrite existing file {}",
            path.display()
        ))
    } else {
        Ok(())
    }
}

pub(super) fn delete_yaml_file(path: &Path) -> Result<PathBuf, String> {
    std::fs::remove_file(path)
        .map(|()| path.to_path_buf())
        .map_err(|e| format!("failed to delete YAML {}: {}", path.display(), e))
}

pub(super) fn delete_file_if_exists(path: &Path) -> Result<PathBuf, String> {
    if !path.exists() {
        return Ok(path.to_path_buf());
    }
    std::fs::remove_file(path)
        .map(|()| path.to_path_buf())
        .map_err(|e| format!("failed to delete file {}: {}", path.display(), e))
}

fn replace_file_atomically(path: &Path, content: &str, label: &str) -> Result<(), String> {
    let tmp = temp_sibling_path(path);
    replace_file_atomically_with_temp(path, &tmp, content, label)
}

fn replace_file_atomically_with_temp(
    path: &Path,
    tmp: &Path,
    content: &str,
    label: &str,
) -> Result<(), String> {
    if let Err(err) = std::fs::write(tmp, content) {
        cleanup_temp_file(tmp);
        return Err(format!(
            "failed to write {} to {}: {}",
            label,
            tmp.display(),
            err
        ));
    }

    let backup = match backup_existing_file(path) {
        Ok(backup) => backup,
        Err(err) => {
            cleanup_temp_file(tmp);
            return Err(err);
        }
    };

    if let Err(err) = try_rename(tmp, path) {
        rollback_replacement(path, backup.as_deref());
        cleanup_temp_file(tmp);
        return Err(err);
    }

    cleanup_backup(backup.as_deref());
    Ok(())
}

fn write_script_provider_files_with_temps(
    script_tmp: &Path,
    yaml_tmp: &Path,
    script_path: &Path,
    yaml_path: &Path,
    script_content: &str,
    yaml_content: &str,
) -> Result<(), String> {
    if let Err(err) = std::fs::write(script_tmp, script_content) {
        cleanup_temp_file(script_tmp);
        cleanup_temp_file(yaml_tmp);
        return Err(format!(
            "failed to write script to {}: {}",
            script_tmp.display(),
            err
        ));
    }
    if let Err(err) = std::fs::write(yaml_tmp, yaml_content) {
        cleanup_temp_file(script_tmp);
        cleanup_temp_file(yaml_tmp);
        return Err(format!(
            "failed to write YAML to {}: {}",
            yaml_tmp.display(),
            err
        ));
    }

    let script_backup = match backup_existing_file(script_path) {
        Ok(backup) => backup,
        Err(err) => {
            cleanup_temp_file(script_tmp);
            cleanup_temp_file(yaml_tmp);
            return Err(err);
        }
    };
    let yaml_backup = match backup_existing_file(yaml_path) {
        Ok(backup) => backup,
        Err(err) => {
            rollback_replacement(script_path, script_backup.as_deref());
            cleanup_temp_file(script_tmp);
            cleanup_temp_file(yaml_tmp);
            return Err(err);
        }
    };

    if let Err(err) = try_rename(script_tmp, script_path) {
        rollback_replacement(script_path, script_backup.as_deref());
        rollback_replacement(yaml_path, yaml_backup.as_deref());
        cleanup_temp_file(script_tmp);
        cleanup_temp_file(yaml_tmp);
        return Err(err);
    }

    if let Err(err) = try_rename(yaml_tmp, yaml_path) {
        rollback_replacement(script_path, script_backup.as_deref());
        rollback_replacement(yaml_path, yaml_backup.as_deref());
        cleanup_temp_file(script_tmp);
        cleanup_temp_file(yaml_tmp);
        return Err(err);
    }

    cleanup_backup(script_backup.as_deref());
    cleanup_backup(yaml_backup.as_deref());
    Ok(())
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

fn rollback_replacement(path: &Path, backup: Option<&Path>) {
    let _ = std::fs::remove_file(path);
    if let Some(backup) = backup {
        let _ = std::fs::rename(backup, path);
    }
}

fn cleanup_backup(backup: Option<&Path>) {
    if let Some(backup) = backup {
        let _ = std::fs::remove_file(backup);
    }
}

fn cleanup_temp_file(path: &Path) {
    let _ = std::fs::remove_file(path);
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn write_newapi_yaml_replaces_existing_file_and_cleans_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("providers").join("relay.yaml");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "old yaml").unwrap();

        write_newapi_yaml(&path, "new yaml").unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new yaml");
        assert_no_backup_files(path.parent().unwrap());
    }

    #[test]
    fn write_newapi_yaml_keeps_old_file_when_temp_write_fails() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("providers").join("relay.yaml");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "old yaml").unwrap();
        let tmp_blocker = path.parent().unwrap().join(".relay.yaml.blocked.tmp");
        std::fs::create_dir(&tmp_blocker).unwrap();

        let err =
            replace_file_atomically_with_temp(&path, &tmp_blocker, "new yaml", "YAML").unwrap_err();

        assert!(err.contains("failed to write YAML"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "old yaml");
    }

    #[test]
    fn ensure_new_file_refuses_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("script.py");
        std::fs::write(&path, "old").unwrap();

        let err = ensure_new_file(&path).unwrap_err();

        assert!(err.contains("refusing to overwrite"));
    }

    #[test]
    fn write_script_provider_files_leaves_old_files_when_yaml_temp_write_fails() {
        let dir = tempfile::tempdir().unwrap();
        let yaml_dir = dir.path().join("providers");
        let script_dir = dir.path().join("scripts");
        std::fs::create_dir_all(&yaml_dir).unwrap();
        std::fs::create_dir_all(&script_dir).unwrap();
        let yaml_path = yaml_dir.join("script-test.yaml");
        let script_path = script_dir.join("script-test.py");
        std::fs::write(&yaml_path, "old yaml").unwrap();
        std::fs::write(&script_path, "old script").unwrap();
        let script_tmp = script_dir.join("script-test.py.tmp");
        let yaml_tmp = yaml_dir.join("script-test.yaml.tmp");
        std::fs::create_dir(&yaml_tmp).unwrap();

        let err = write_script_provider_files_with_temps(
            &script_tmp,
            &yaml_tmp,
            &script_path,
            &yaml_path,
            "new script",
            "new yaml",
        )
        .unwrap_err();

        assert!(err.contains("failed to write YAML"));
        assert_eq!(std::fs::read_to_string(&yaml_path).unwrap(), "old yaml");
        assert_eq!(std::fs::read_to_string(&script_path).unwrap(), "old script");
        assert!(!script_tmp.exists());
    }

    #[test]
    fn write_script_provider_files_rolls_back_existing_script_when_yaml_rename_fails() {
        let dir = tempfile::tempdir().unwrap();
        let script_dir = dir.path().join("scripts");
        std::fs::create_dir_all(&script_dir).unwrap();
        let script_path = script_dir.join("test.py");
        std::fs::write(&script_path, "old script").unwrap();
        let script_tmp = script_dir.join("test.py.tmp");
        let yaml_tmp = dir.path().join("test.yaml.tmp");
        let yaml_path = dir.path().join("nonexistent_dir").join("test.yaml");

        let err = write_script_provider_files_with_temps(
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
        assert!(!script_tmp.exists());
        assert!(!yaml_tmp.exists());
        assert!(!yaml_path.exists());
    }

    #[test]
    fn write_script_provider_files_removes_created_script_when_yaml_rename_fails() {
        let dir = tempfile::tempdir().unwrap();
        let script_dir = dir.path().join("scripts");
        std::fs::create_dir_all(&script_dir).unwrap();
        let script_path = script_dir.join("test.py");
        let script_tmp = script_dir.join("test.py.tmp");
        let yaml_tmp = dir.path().join("test.yaml.tmp");
        let yaml_path = dir.path().join("nonexistent_dir").join("test.yaml");

        let err = write_script_provider_files_with_temps(
            &script_tmp,
            &yaml_tmp,
            &script_path,
            &yaml_path,
            "new script",
            "new yaml",
        )
        .unwrap_err();

        assert!(err.contains("failed to replace"));
        assert!(!script_path.exists());
        assert!(!script_tmp.exists());
        assert!(!yaml_tmp.exists());
    }

    #[test]
    fn write_script_provider_files_replaces_both_files_and_cleans_backups() {
        let dir = tempfile::tempdir().unwrap();
        let yaml_path = dir.path().join("providers").join("script.yaml");
        let script_path = dir.path().join("scripts").join("script.py");
        std::fs::create_dir_all(yaml_path.parent().unwrap()).unwrap();
        std::fs::create_dir_all(script_path.parent().unwrap()).unwrap();
        std::fs::write(&yaml_path, "old yaml").unwrap();
        std::fs::write(&script_path, "old script").unwrap();

        write_script_provider_files(&script_path, &yaml_path, "new script", "new yaml").unwrap();

        assert_eq!(std::fs::read_to_string(&script_path).unwrap(), "new script");
        assert_eq!(std::fs::read_to_string(&yaml_path).unwrap(), "new yaml");
        assert_no_backup_files(script_path.parent().unwrap());
        assert_no_backup_files(yaml_path.parent().unwrap());
    }

    #[test]
    fn delete_file_if_exists_reports_delete_failure() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("script-dir");
        std::fs::create_dir(&path).unwrap();

        let err = delete_file_if_exists(&path).unwrap_err();

        assert!(err.contains("failed to delete file"));
    }

    fn assert_no_backup_files(dir: &Path) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let filename = entry.unwrap().file_name();
            assert!(
                !filename.to_string_lossy().contains(".bak"),
                "unexpected backup file: {}",
                filename.to_string_lossy()
            );
        }
    }
}
