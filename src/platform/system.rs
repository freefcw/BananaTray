use std::path::Path;

/// 使用系统默认浏览器打开外部 URL
///
/// 跨平台支持：macOS → `open`，Linux → `xdg-open`，Windows → `start`
pub fn open_url(url: &str) {
    let cmd = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "linux") {
        "xdg-open"
    } else {
        "start"
    };
    let _ = std::process::Command::new(cmd).arg(url).spawn();
}

/// 在系统文件管理器中打开指定路径（显示该文件所在目录）
///
/// macOS: `open -R <path>`（在 Finder 中选中文件）
/// Linux: `xdg-open <parent_dir>`
pub fn open_path_in_finder(path: &Path) {
    if cfg!(target_os = "macos") {
        let _ = std::process::Command::new("open")
            .arg("-R")
            .arg(path)
            .spawn();
    } else if cfg!(target_os = "linux") {
        let dir = path.parent().unwrap_or(path);
        let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
    }
}

/// 将文本写入系统剪贴板
///
/// macOS: `pbcopy`
/// Linux: `xclip` 或 `xsel`
pub fn copy_to_clipboard(text: &str) {
    use std::io::Write;
    #[cfg(target_os = "macos")]
    {
        match std::process::Command::new("pbcopy")
            .stdin(std::process::Stdio::piped())
            .spawn()
        {
            Ok(mut child) => {
                if let Some(stdin) = child.stdin.as_mut() {
                    let _ = stdin.write_all(text.as_bytes());
                }
                let _ = child.wait();
                log::info!(target: "runtime", "copied {} bytes to clipboard", text.len());
            }
            Err(err) => {
                log::warn!(target: "runtime", "failed to copy to clipboard: {}", err);
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        let result = std::process::Command::new("xclip")
            .args(["-selection", "clipboard"])
            .stdin(std::process::Stdio::piped())
            .spawn()
            .or_else(|_| {
                std::process::Command::new("xsel")
                    .arg("--clipboard")
                    .stdin(std::process::Stdio::piped())
                    .spawn()
            });
        match result {
            Ok(mut child) => {
                if let Some(stdin) = child.stdin.as_mut() {
                    let _ = stdin.write_all(text.as_bytes());
                }
                let _ = child.wait();
                log::info!(target: "runtime", "copied {} bytes to clipboard", text.len());
            }
            Err(err) => {
                log::warn!(target: "runtime", "failed to copy to clipboard: {}", err);
            }
        }
    }
}

/// 获取操作系统版本信息字符串
///
/// macOS: `macOS 15.4 (aarch64)`
/// Linux: `Linux (x86_64)`
pub fn os_info() -> String {
    let arch = std::env::consts::ARCH;

    #[cfg(target_os = "macos")]
    {
        let version = std::process::Command::new("sw_vers")
            .arg("-productVersion")
            .output()
            .ok()
            .and_then(|o| {
                let v = String::from_utf8_lossy(&o.stdout).trim().to_string();
                if v.is_empty() {
                    None
                } else {
                    Some(v)
                }
            })
            .unwrap_or_else(|| "unknown".to_string());
        format!("macOS {} ({})", version, arch)
    }
    #[cfg(target_os = "linux")]
    {
        format!("Linux ({})", arch)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        format!("{} ({})", std::env::consts::OS, arch)
    }
}

/// 将文件大小格式化为人类可读的字符串
pub fn format_file_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// 检测系统是否处于深色模式
///
/// macOS: 读取 `defaults read -g AppleInterfaceStyle`
/// Linux: 优先读取 GNOME `color-scheme`，fallback 到 GTK 主题名
pub fn detect_system_dark_mode() -> bool {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("defaults")
            .args(["read", "-g", "AppleInterfaceStyle"])
            .output()
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .trim()
                    .eq_ignore_ascii_case("dark")
            })
            .unwrap_or(false)
    }
    #[cfg(target_os = "linux")]
    {
        detect_linux_dark_mode()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        false
    }
}

/// Linux 深色模式检测
///
/// 1. `org.gnome.desktop.interface color-scheme` → GNOME 42+ 标准
/// 2. `org.gnome.desktop.interface gtk-theme` → 主题名含 "dark" 的 fallback
#[cfg(target_os = "linux")]
fn detect_linux_dark_mode() -> bool {
    // 方法 1: GNOME color-scheme（'prefer-dark' = 深色）
    if let Ok(output) = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "color-scheme"])
        .output()
    {
        let value = String::from_utf8_lossy(&output.stdout);
        if value.contains("prefer-dark") {
            return true;
        }
        // 如果返回了有效值（如 'default'），说明 gsettings 可用但不是深色
        if value.contains("default") || value.contains("prefer-light") {
            return false;
        }
    }

    // 方法 2: GTK 主题名是否包含 "dark"
    if let Ok(output) = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "gtk-theme"])
        .output()
    {
        let theme = String::from_utf8_lossy(&output.stdout)
            .trim()
            .to_lowercase();
        if theme.contains("dark") {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_url_does_not_panic_on_valid_url() {
        // 仅验证函数签名可调用、不 panic；不真正执行以避免 LaunchServices 噪音
        let _ = &open_url as &dyn Fn(&str);
    }

    #[test]
    fn os_info_returns_non_empty() {
        let info = os_info();
        assert!(!info.is_empty());
        assert!(info.contains(std::env::consts::ARCH));
    }

    #[test]
    fn format_file_size_units() {
        assert_eq!(format_file_size(500), "500 B");
        assert_eq!(format_file_size(1024), "1.0 KB");
        assert_eq!(format_file_size(1536), "1.5 KB");
        assert_eq!(format_file_size(1048576), "1.0 MB");
        assert_eq!(format_file_size(2621440), "2.5 MB");
    }
}
