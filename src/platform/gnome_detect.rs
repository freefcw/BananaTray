//! GNOME 桌面 + BananaTray 扩展检测
//!
//! 判断当前是否应使用原生 GNOME Shell Extension 模式：
//! 1. 当前桌面环境是 GNOME
//! 2. BananaTray 扩展已安装 **且已启用**
//!
//! 如果扩展仅安装但未启用/加载失败，不应跳过 KSNI 菜单 fallback，
//! 否则用户会丢失右键菜单入口。

/// GNOME Shell Extension UUID
const EXTENSION_UUID: &str = "bananatray@bananatray.github.io";

/// 判断是否应使用 GNOME Shell Extension 模式
///
/// 条件：GNOME 桌面 AND 扩展已安装且已启用。
/// 仅"已安装"不够——扩展可能被禁用、加载失败或版本不兼容。
pub fn should_use_gnome_extension() -> bool {
    if !is_gnome_desktop() {
        return false;
    }
    if !is_extension_enabled() {
        return false;
    }
    true
}

/// 检测当前桌面环境是否为 GNOME
fn is_gnome_desktop() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP")
        .map(|v| v.to_lowercase().contains("gnome"))
        .unwrap_or(false)
}

/// 检测 BananaTray GNOME Shell Extension 是否已启用
///
/// 优先使用 `gnome-extensions list --enabled`，这是最可靠的检测方式。
/// 如果 CLI 不可用，回退到检查扩展目录是否存在（保守策略：保留 KSNI fallback）。
fn is_extension_enabled() -> bool {
    // 首选：通过 gnome-extensions CLI 检查是否在 --enabled 列表中
    if is_extension_enabled_via_cli() {
        return true;
    }

    // 如果 CLI 执行成功但扩展不在 --enabled 列表中，返回 false
    // （不回退到"已安装"检测——仅安装不够，必须确认已启用）

    false
}

/// 通过 gnome-extensions CLI 检查扩展是否已启用
///
/// 使用 `gnome-extensions list --enabled` 而非 `gnome-extensions list`，
/// 确保只匹配已启用（而非仅已安装）的扩展。
fn is_extension_enabled_via_cli() -> bool {
    let output = std::process::Command::new("gnome-extensions")
        .args(["list", "--enabled"])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            stdout.lines().any(|line| line.trim() == EXTENSION_UUID)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gnome_desktop_detection_checks_xdg() {
        let result = is_gnome_desktop();
        assert!(
            std::env::var("XDG_CURRENT_DESKTOP")
                .map(|v| v.to_lowercase().contains("gnome"))
                .unwrap_or(false)
                == result
        );
    }

    #[test]
    fn extension_uuid_is_valid() {
        assert!(EXTENSION_UUID.contains('@'));
        assert!(EXTENSION_UUID.contains('.'));
    }
}
