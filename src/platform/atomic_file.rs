//! 私有配置与凭证文件的原子写入原语。

use std::io::{self, Write};
use std::path::Path;

#[cfg(any(feature = "app", test))]
use std::fs::OpenOptions;
#[cfg(all(unix, any(feature = "app", test)))]
use std::os::unix::fs::OpenOptionsExt;
#[cfg(all(unix, feature = "app"))]
use std::os::unix::fs::PermissionsExt;

/// 将私有内容写入目标文件，并通过同目录 `rename` 原子替换旧内容。
///
/// 临时文件使用唯一名称且在 Unix 上以 `0600` 创建。写入或同步失败时旧文件保持不变，
/// 可恢复错误路径由 `NamedTempFile` 自动清理临时文件。
pub(crate) fn write_private_file_atomically(path: &Path, contents: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("atomic write target has no parent: {}", path.display()),
            )
        })?;
    let filename = path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("atomic write target has no file name: {}", path.display()),
        )
    })?;
    let prefix = format!(".{}.", filename.to_string_lossy());
    let mut temp = tempfile::Builder::new()
        .prefix(&prefix)
        .suffix(".tmp")
        .tempfile_in(parent)
        .map_err(|error| contextualize("create sibling temp for private file", path, error))?;

    temp.write_all(contents)
        .map_err(|error| contextualize("write private temp for", path, error))?;
    temp.as_file()
        .sync_all()
        .map_err(|error| contextualize("sync private temp for", path, error))?;
    temp.persist(path)
        .map(|_| ())
        .map_err(|error| contextualize("replace private file", path, error.error))
}

/// 创建新的私密文件并同步内容，拒绝覆盖已有路径。
///
/// 供需要自行编排多文件提交/回滚的调用方写入 sibling temp。调用方在成功后拥有该
/// 临时文件，并负责 rename 或清理；本函数会清理由写入或同步失败产生的半成品。
#[cfg(any(feature = "app", test))]
pub(crate) fn write_private_file_exclusively(path: &Path, contents: &[u8]) -> io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);

    let mut file = options
        .open(path)
        .map_err(|error| contextualize("create private file", path, error))?;

    let result = file
        .write_all(contents)
        .and_then(|()| file.sync_all())
        .map_err(|error| contextualize("write private file", path, error));
    if result.is_err() {
        drop(file);
        let _ = std::fs::remove_file(path);
    }
    result
}

/// 收紧已有私密文件权限，避免备份/回滚窗口继续暴露旧凭证。
#[cfg(feature = "app")]
pub(crate) fn restrict_private_file_permissions(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        let metadata = std::fs::symlink_metadata(path)?;
        if metadata.file_type().is_file() {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        }
    }
    #[cfg(not(unix))]
    let _ = path;

    Ok(())
}

fn contextualize(operation: &str, path: &Path, error: io::Error) -> io::Error {
    io::Error::new(
        error.kind(),
        format!("{operation} {}: {error}", path.display()),
    )
}

#[cfg(test)]
mod tests {
    use super::{write_private_file_atomically, write_private_file_exclusively};
    use std::sync::{Arc, Barrier};

    #[test]
    fn replaces_existing_file_without_leaving_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, b"old").unwrap();

        write_private_file_atomically(&path, b"new").unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), b"new");
        let entries = std::fs::read_dir(dir.path()).unwrap().count();
        assert_eq!(
            entries, 1,
            "successful replacement should clean its temp file"
        );
    }

    #[test]
    fn concurrent_writes_use_independent_temp_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth.json");
        let payloads = (0..4)
            .map(|index| format!("payload-{index}-{}", "x".repeat(64 * 1024)).into_bytes())
            .collect::<Vec<_>>();
        let barrier = Arc::new(Barrier::new(payloads.len()));

        std::thread::scope(|scope| {
            let handles = payloads
                .iter()
                .map(|payload| {
                    let path = path.clone();
                    let barrier = Arc::clone(&barrier);
                    scope.spawn(move || {
                        barrier.wait();
                        write_private_file_atomically(&path, payload)
                    })
                })
                .collect::<Vec<_>>();

            for handle in handles {
                handle.join().unwrap().unwrap();
            }
        });

        let persisted = std::fs::read(&path).unwrap();
        assert!(payloads.contains(&persisted));
        let entries = std::fs::read_dir(dir.path()).unwrap().count();
        assert_eq!(entries, 1, "concurrent writes should not leave temp files");
    }

    #[test]
    fn failed_replacement_cleans_up_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("occupied");
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join("child"), b"keep directory non-empty").unwrap();

        let error = write_private_file_atomically(&path, b"new").unwrap_err();

        let message = error.to_string();
        assert!(message.contains("replace private file"));
        assert!(message.contains(&path.display().to_string()));
        let entries = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        assert_eq!(entries, vec!["occupied"]);
    }

    #[cfg(unix)]
    #[test]
    fn creates_private_file_with_owner_only_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credentials.json");

        write_private_file_atomically(&path, b"secret").unwrap();

        let mode = std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn exclusive_private_file_uses_owner_only_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("provider.yaml.tmp");

        write_private_file_exclusively(&path, b"secret").unwrap();

        let mode = std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn exclusive_private_file_refuses_to_overwrite_existing_contents() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("provider.yaml.tmp");
        std::fs::write(&path, b"existing secret").unwrap();

        let error = write_private_file_exclusively(&path, b"replacement").unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(path).unwrap(), b"existing secret");
    }
}
