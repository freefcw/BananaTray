use std::borrow::Cow;
use std::fs;
use std::path::PathBuf;

use gpui::{AssetSource, SharedString};

pub(crate) struct Assets {
    base: PathBuf,
}

impl Assets {
    pub fn new() -> Self {
        Self {
            base: Self::resolve_base(),
        }
    }

    /// 解析资源根目录（按优先级）：
    /// 1. 环境变量 BANANATRAY_RESOURCES（AppImage 通过 AppRun 设置）
    /// 2. macOS: .app/Contents/Resources/
    /// 3. Linux: /usr/share/bananatray（deb 安装路径）
    /// 4. 开发模式: CARGO_MANIFEST_DIR
    fn resolve_base() -> PathBuf {
        if let Some(path) = Self::from_env() {
            return path;
        }
        if let Some(path) = Self::from_bundle() {
            return path;
        }
        if let Some(path) = Self::from_system() {
            return path;
        }
        Self::from_dev()
    }

    /// AppImage: AppRun 设置 BANANATRAY_RESOURCES 环境变量
    fn from_env() -> Option<PathBuf> {
        let dir = std::env::var("BANANATRAY_RESOURCES").ok()?;
        let path = PathBuf::from(dir);
        if path.is_dir() {
            log::info!(target: "assets", "using BANANATRAY_RESOURCES: {}", path.display());
            Some(path)
        } else {
            None
        }
    }

    /// macOS: .app/Contents/MacOS/bananatray -> .app/Contents/Resources/
    fn from_bundle() -> Option<PathBuf> {
        let exe = std::env::current_exe().ok()?;
        let macos_dir = exe.parent()?;
        let contents_dir = macos_dir.parent()?;
        let resources_dir = contents_dir.join("Resources");
        if resources_dir.is_dir() {
            log::info!(target: "assets", "using bundle resources: {}", resources_dir.display());
            Some(resources_dir)
        } else {
            None
        }
    }

    /// Linux deb: /usr/share/bananatray
    fn from_system() -> Option<PathBuf> {
        let path = PathBuf::from("/usr/share/bananatray");
        if path.is_dir() {
            log::info!(target: "assets", "using system resources: {}", path.display());
            Some(path)
        } else {
            None
        }
    }

    /// 开发模式回退
    fn from_dev() -> PathBuf {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        log::info!(target: "assets", "using dev resources: {}", path.display());
        path
    }
}

impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        fs::read(self.base.join(path))
            .map(|data| Some(Cow::Owned(data)))
            .map_err(|err| err.into())
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        fs::read_dir(self.base.join(path))
            .map(|entries| {
                entries
                    .filter_map(|entry| {
                        entry
                            .ok()
                            .and_then(|entry| entry.file_name().into_string().ok())
                            .map(SharedString::from)
                    })
                    .collect()
            })
            .map_err(|err| err.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        ENV_LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn test_from_env_with_valid_dir() {
        let _guard = env_lock().lock().unwrap();
        // 使用临时目录模拟 BANANATRAY_RESOURCES
        let tmp = env::temp_dir().join("bananatray_test_env");
        fs::create_dir_all(&tmp).unwrap();

        // SAFETY: 测试串行执行（cargo test 默认单线程），无并发 env 访问
        unsafe { env::set_var("BANANATRAY_RESOURCES", tmp.to_str().unwrap()) };
        let result = Assets::from_env();
        assert!(result.is_some());
        assert_eq!(result.unwrap(), tmp);

        unsafe { env::remove_var("BANANATRAY_RESOURCES") };
        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_from_env_with_nonexistent_dir() {
        let _guard = env_lock().lock().unwrap();
        // SAFETY: 测试串行执行，无并发 env 访问
        unsafe { env::set_var("BANANATRAY_RESOURCES", "/nonexistent/path/bananatray") };
        let result = Assets::from_env();
        assert!(result.is_none());
        unsafe { env::remove_var("BANANATRAY_RESOURCES") };
    }

    #[test]
    fn test_from_env_unset() {
        let _guard = env_lock().lock().unwrap();
        // SAFETY: 测试串行执行，无并发 env 访问
        unsafe { env::remove_var("BANANATRAY_RESOURCES") };
        let result = Assets::from_env();
        assert!(result.is_none());
    }

    #[test]
    fn test_from_system_nonexistent() {
        // /usr/share/bananatray 通常在开发环境不存在
        let result = Assets::from_system();
        // 在开发机上应该为 None（除非安装过 deb）
        if !PathBuf::from("/usr/share/bananatray").is_dir() {
            assert!(result.is_none());
        }
    }

    #[test]
    fn test_from_dev_returns_manifest_dir() {
        let result = Assets::from_dev();
        assert!(
            result.is_dir(),
            "CARGO_MANIFEST_DIR should exist: {:?}",
            result
        );
        assert!(result.join("Cargo.toml").exists());
    }

    #[test]
    fn test_resolve_base_fallback_to_dev() {
        let _guard = env_lock().lock().unwrap();
        // 确保没有干扰环境变量
        // SAFETY: 测试串行执行，无并发 env 访问
        unsafe { env::remove_var("BANANATRAY_RESOURCES") };
        let result = Assets::resolve_base();
        // 在开发环境中，应该回退到 CARGO_MANIFEST_DIR
        assert!(result.is_dir());
        assert!(result.join("Cargo.toml").exists() || result.join("src").exists());
    }
}
