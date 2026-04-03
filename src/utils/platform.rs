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

/// 检测系统是否处于深色模式
///
/// macOS: 读取 `defaults read -g AppleInterfaceStyle`
/// Linux/其他: 默认浅色模式
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
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_url_does_not_panic_on_valid_url() {
        // 仅验证不 panic，不验证是否真正打开（需要桌面环境）
        // 使用 about:blank 避免实际打开浏览器在 CI 环境
        // 注意：此测试在无桌面环境中会静默失败（spawn 返回 Err），这是预期行为
        open_url("about:blank");
    }
}
