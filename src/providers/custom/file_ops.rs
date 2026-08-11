//! Low-level custom provider file operations.
//!
//! This module owns atomic-ish sibling-temp replacement, backup/restore, and
//! delete primitives. Provider identity, YAML parsing, and user-facing lifecycle
//! decisions belong in `newapi_lifecycle.rs` / `script_provider_lifecycle.rs`.

use std::path::{Path, PathBuf};

use crate::platform::atomic_file::write_private_file_exclusively;

pub(super) fn write_newapi_yaml(path: &Path, yaml_content: &str) -> Result<PathBuf, String> {
    let providers_dir = path
        .parent()
        .ok_or_else(|| format!("failed to resolve providers dir for {}", path.display()))?;
    std::fs::create_dir_all(providers_dir)
        .map_err(|e| format!("failed to create providers dir: {}", e))?;

    replace_file_atomically(path, yaml_content, "YAML")?;
    Ok(path.to_path_buf())
}

/// 提交一个新的不可变脚本版本，再以 YAML rename 作为最终提交点。
///
/// `script_path` 必须是尚不存在的版本化路径。进程在 YAML 提交前退出时旧 YAML
/// 仍然可见；在 YAML 提交后退出时，新 YAML 引用的脚本已经完整落盘。
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

    ensure_new_file(script_path)?;
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
    if let Err(err) = write_private_file_exclusively(tmp, content.as_bytes()) {
        return Err(format!(
            "failed to write {} to {}: {}",
            label,
            tmp.display(),
            err
        ));
    }

    // macOS/Linux 的同目录 rename 会原子替换目标；提交前保持旧文件可见，
    // 进程在写临时文件与 rename 之间退出时，下一次启动仍能加载旧配置。
    if let Err(err) = try_rename(tmp, path) {
        cleanup_temp_file(tmp);
        return Err(err);
    }

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
    if script_path.exists() {
        return Err(format!(
            "refusing to overwrite immutable script version {}",
            script_path.display()
        ));
    }
    if let Err(err) = write_private_file_exclusively(script_tmp, script_content.as_bytes()) {
        return Err(format!(
            "failed to write script to {}: {}",
            script_tmp.display(),
            err
        ));
    }
    if let Err(err) = write_private_file_exclusively(yaml_tmp, yaml_content.as_bytes()) {
        cleanup_temp_file(script_tmp);
        return Err(format!(
            "failed to write YAML to {}: {}",
            yaml_tmp.display(),
            err
        ));
    }

    if let Err(err) = try_rename(script_tmp, script_path) {
        cleanup_temp_file(script_tmp);
        cleanup_temp_file(yaml_tmp);
        return Err(err);
    }

    // YAML 是提交点；失败时删除尚未被任何生效配置引用的新脚本。
    if let Err(err) = try_rename(yaml_tmp, yaml_path) {
        cleanup_temp_file(script_path);
        cleanup_temp_file(yaml_tmp);
        return Err(err);
    }

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

pub(super) fn versioned_script_path(path: &Path) -> PathBuf {
    let stem = path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("script");
    let stable_stem = stem
        .rsplit_once('.')
        .filter(|(_, suffix)| suffix.chars().all(|ch| ch.is_ascii_digit()))
        .map(|(base, _)| base)
        .unwrap_or(stem);
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("py");
    path.with_file_name(format!(
        "{}.{}.{}",
        stable_stem,
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default(),
        extension
    ))
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

    #[cfg(unix)]
    #[test]
    fn write_newapi_yaml_restricts_existing_file_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("providers").join("relay.yaml");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "old yaml").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

        write_newapi_yaml(&path, "new yaml").unwrap();

        assert_eq!(file_mode(&path), 0o600);
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
    fn write_script_provider_files_leaves_active_files_when_yaml_temp_write_fails() {
        let dir = tempfile::tempdir().unwrap();
        let yaml_dir = dir.path().join("providers");
        let script_dir = dir.path().join("scripts");
        std::fs::create_dir_all(&yaml_dir).unwrap();
        std::fs::create_dir_all(&script_dir).unwrap();
        let yaml_path = yaml_dir.join("script-test.yaml");
        let old_script_path = script_dir.join("script-test.1.py");
        let new_script_path = script_dir.join("script-test.2.py");
        std::fs::write(&yaml_path, "old yaml").unwrap();
        std::fs::write(&old_script_path, "old script").unwrap();
        let script_tmp = script_dir.join("script-test.2.py.tmp");
        let yaml_tmp = yaml_dir.join("script-test.yaml.tmp");
        std::fs::create_dir(&yaml_tmp).unwrap();

        let err = write_script_provider_files_with_temps(
            &script_tmp,
            &yaml_tmp,
            &new_script_path,
            &yaml_path,
            "new script",
            "new yaml",
        )
        .unwrap_err();

        assert!(err.contains("failed to write YAML"));
        assert_eq!(std::fs::read_to_string(&yaml_path).unwrap(), "old yaml");
        assert_eq!(
            std::fs::read_to_string(&old_script_path).unwrap(),
            "old script"
        );
        assert!(!new_script_path.exists());
        assert!(!script_tmp.exists());
    }

    #[test]
    fn write_script_provider_files_refuses_existing_immutable_version() {
        let dir = tempfile::tempdir().unwrap();
        let script_path = dir.path().join("test.1.py");
        let yaml_path = dir.path().join("test.yaml");
        std::fs::write(&script_path, "old script").unwrap();

        let error = write_script_provider_files(&script_path, &yaml_path, "new script", "new yaml")
            .unwrap_err();

        assert!(error.contains("refusing to overwrite"));
        assert_eq!(std::fs::read_to_string(script_path).unwrap(), "old script");
    }

    #[test]
    fn write_script_provider_files_removes_new_version_when_yaml_rename_fails() {
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
    fn write_script_provider_files_commits_new_script_before_yaml() {
        let dir = tempfile::tempdir().unwrap();
        let yaml_path = dir.path().join("providers").join("script.yaml");
        let old_script_path = dir.path().join("scripts").join("script.1.py");
        let new_script_path = dir.path().join("scripts").join("script.2.py");
        std::fs::create_dir_all(yaml_path.parent().unwrap()).unwrap();
        std::fs::create_dir_all(new_script_path.parent().unwrap()).unwrap();
        std::fs::write(&yaml_path, "old yaml").unwrap();
        std::fs::write(&old_script_path, "old script").unwrap();

        write_script_provider_files(&new_script_path, &yaml_path, "new script", "new yaml")
            .unwrap();

        assert_eq!(
            std::fs::read_to_string(&new_script_path).unwrap(),
            "new script"
        );
        assert_eq!(std::fs::read_to_string(&yaml_path).unwrap(), "new yaml");
        assert_eq!(
            std::fs::read_to_string(&old_script_path).unwrap(),
            "old script"
        );
        assert_no_backup_files(new_script_path.parent().unwrap());
        assert_no_backup_files(yaml_path.parent().unwrap());
    }

    #[test]
    fn versioned_script_path_reuses_stable_stem() {
        let first = Path::new("/tmp/script-demo.py");
        let second = Path::new("/tmp/script-demo.123456.py");

        let first_version = versioned_script_path(first);
        let second_version = versioned_script_path(second);

        assert!(first_version
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("script-demo."));
        assert!(second_version
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("script-demo."));
        assert!(!second_version
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("script-demo.123456."));
    }

    #[cfg(unix)]
    #[test]
    fn write_script_provider_files_create_owner_only_files() {
        let dir = tempfile::tempdir().unwrap();
        let yaml_path = dir.path().join("providers").join("script.yaml");
        let script_path = dir.path().join("scripts").join("script.py");

        write_script_provider_files(&script_path, &yaml_path, "secret script", "secret yaml")
            .unwrap();

        assert_eq!(file_mode(&script_path), 0o600);
        assert_eq!(file_mode(&yaml_path), 0o600);
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

    #[cfg(unix)]
    fn file_mode(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;

        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }
}
