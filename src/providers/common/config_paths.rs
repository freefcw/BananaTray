use std::path::{Path, PathBuf};

/// 构建平台标准 config 目录 + `~/.config` fallback 候选路径。
///
/// `dirs::config_dir()` 会随平台返回不同根目录；fallback 用来覆盖 XDG 环境变量
/// 在 BananaTray 与目标应用进程之间不一致时，应用仍写入 `~/.config` 的情况。
pub(crate) fn config_dir_with_xdg_fallback(relative_path: impl AsRef<Path>) -> Vec<PathBuf> {
    build_config_dir_with_xdg_fallback(dirs::config_dir(), dirs::home_dir(), relative_path.as_ref())
}

fn build_config_dir_with_xdg_fallback(
    config_dir: Option<PathBuf>,
    home_dir: Option<PathBuf>,
    relative_path: &Path,
) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(dir) = config_dir {
        push_unique(&mut candidates, dir.join(relative_path));
    }

    if let Some(home) = home_dir {
        push_unique(&mut candidates, home.join(".config").join(relative_path));
    }

    candidates
}

fn push_unique(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.contains(&path) {
        paths.push(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RELATIVE_PATH: &str = "Example/User/globalStorage/state.vscdb";

    #[test]
    fn builds_primary_and_xdg_fallback() {
        let candidates = build_config_dir_with_xdg_fallback(
            Some(PathBuf::from("/custom/config")),
            Some(PathBuf::from("/home/user")),
            Path::new(RELATIVE_PATH),
        );

        assert_eq!(candidates.len(), 2, "should have primary + XDG fallback");
        assert_eq!(
            candidates[0],
            PathBuf::from("/custom/config").join(RELATIVE_PATH)
        );
        assert_eq!(
            candidates[1],
            PathBuf::from("/home/user/.config").join(RELATIVE_PATH)
        );
    }

    #[test]
    fn builds_primary_only_when_home_missing() {
        let candidates = build_config_dir_with_xdg_fallback(
            Some(PathBuf::from("/custom/config")),
            None,
            Path::new(RELATIVE_PATH),
        );

        assert_eq!(
            candidates,
            vec![PathBuf::from("/custom/config").join(RELATIVE_PATH)]
        );
    }

    #[test]
    fn builds_xdg_fallback_only_when_config_missing() {
        let candidates = build_config_dir_with_xdg_fallback(
            None,
            Some(PathBuf::from("/home/user")),
            Path::new(RELATIVE_PATH),
        );

        assert_eq!(
            candidates,
            vec![PathBuf::from("/home/user/.config").join(RELATIVE_PATH)]
        );
    }

    #[test]
    fn returns_empty_when_no_roots_are_available() {
        let candidates = build_config_dir_with_xdg_fallback(None, None, Path::new(RELATIVE_PATH));

        assert!(candidates.is_empty());
    }

    #[test]
    fn deduplicates_linux_default_config_dir() {
        let candidates = build_config_dir_with_xdg_fallback(
            Some(PathBuf::from("/home/user/.config")),
            Some(PathBuf::from("/home/user")),
            Path::new(RELATIVE_PATH),
        );

        assert_eq!(candidates.len(), 1, "Linux default should deduplicate");
        assert_eq!(
            candidates[0],
            PathBuf::from("/home/user/.config").join(RELATIVE_PATH)
        );
    }

    #[test]
    fn keeps_macos_application_support_and_xdg_candidates() {
        let candidates = build_config_dir_with_xdg_fallback(
            Some(PathBuf::from("/Users/user/Library/Application Support")),
            Some(PathBuf::from("/Users/user")),
            Path::new(RELATIVE_PATH),
        );

        assert_eq!(candidates.len(), 2, "macOS should keep both candidates");
    }
}
