//! GNOME 桌面 + BananaTray 扩展检测
//!
//! 判断当前是否应使用原生 GNOME Shell Extension 模式：
//! 1. 当前桌面环境是 GNOME
//! 2. BananaTray 扩展已安装、已启用，且 GNOME Shell 实际加载为 ACTIVE
//!
//! 如果扩展仅安装/启用但版本不兼容或加载失败，不应跳过 KSNI 菜单 fallback，
//! 否则用户会丢失传统托盘入口。

/// GNOME Shell Extension UUID
const EXTENSION_UUID: &str = "bananatray@bananatray.github.io";
const FORCE_EXTENSION_ENV: &str = "BANANATRAY_FORCE_GNOME_EXTENSION";
#[cfg(test)]
const EXTENSION_TEST_OVERRIDE_ENV: &str = "BANANATRAY_TEST_GNOME_EXTENSION_ENABLED";

/// 判断是否应使用 GNOME Shell Extension 模式
///
/// 条件：GNOME 桌面 AND 扩展已启用且处于 ACTIVE 状态。
/// 仅"已启用"不够——扩展可能版本不兼容（OUT OF DATE）或加载失败。
pub fn should_use_gnome_extension() -> bool {
    if env_flag_enabled(FORCE_EXTENSION_ENV) {
        return true;
    }

    #[cfg(test)]
    if let Some(enabled) = test_override_enabled() {
        return enabled;
    }

    if !is_gnome_desktop() {
        return false;
    }
    if !is_extension_active() {
        return false;
    }
    true
}

/// 在进入 GPUI 事件循环前执行一次有 500ms 上限的探测。
///
/// 之后前台调用只读缓存；TTL 到期后的刷新也在后台执行。
#[cfg(target_os = "linux")]
#[allow(dead_code)] // app 启动前 wiring 调用；非 Linux/lib-only 目标不会触达
pub(crate) fn prewarm_gnome_extension_detection() {
    if is_gnome_desktop() {
        extension_state_probe().publish(is_extension_active_via_cli());
    }
}

fn env_flag_enabled(name: &str) -> bool {
    std::env::var(name)
        .map(|value| env_flag_value_enabled(&value))
        .unwrap_or(false)
}

#[cfg(test)]
fn test_override_enabled() -> Option<bool> {
    std::env::var(EXTENSION_TEST_OVERRIDE_ENV)
        .ok()
        .map(|value| env_flag_value_enabled(&value))
}

fn env_flag_value_enabled(value: &str) -> bool {
    matches!(value, "1" | "true" | "TRUE" | "yes" | "YES")
}

/// 检测当前桌面环境是否为 GNOME
fn is_gnome_desktop() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP")
        .map(|v| v.to_lowercase().contains("gnome"))
        .unwrap_or(false)
}

/// 检测 BananaTray GNOME Shell Extension 是否已被 GNOME Shell 实际加载
///
/// `gnome-extensions list --enabled` 只能证明用户打开了开关；
/// `info` 的 `State: ACTIVE` 才能证明 Shell 已成功加载扩展。
///
/// 结果带 TTL 缓存：该函数会在每次托盘图标更新时被调用（UI 线程），
/// 实时 spawn `gnome-extensions` 子进程代价过高；代价是扩展状态变化
/// （启用/禁用/重载）最长延迟 `EXTENSION_STATE_CACHE_TTL` 才被感知。
fn is_extension_active() -> bool {
    extension_state_probe().get_or_refresh(
        false,
        EXTENSION_STATE_CACHE_TTL,
        is_extension_active_via_cli,
    )
}

fn extension_state_probe() -> &'static crate::platform::system::NonBlockingBoolProbe {
    static PROBE: std::sync::OnceLock<crate::platform::system::NonBlockingBoolProbe> =
        std::sync::OnceLock::new();
    PROBE.get_or_init(crate::platform::system::NonBlockingBoolProbe::default)
}

/// CLI 检测结果缓存 TTL：每次托盘图标更新都会触发检测，
/// 缓存避免在 UI 线程上频繁 spawn 子进程。
const EXTENSION_STATE_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(30);

/// 通过 gnome-extensions CLI 检查扩展是否处于 ACTIVE 状态
///
/// 使用 `LC_ALL=C` 固定输出语言，避免本地化文案影响解析。
fn is_extension_active_via_cli() -> bool {
    let mut command = std::process::Command::new("gnome-extensions");
    command
        .args(["info", EXTENSION_UUID])
        .env("LC_ALL", "C")
        .env(
            "PATH",
            crate::providers::common::path_resolver::enriched_path(),
        );
    let output = crate::providers::common::cli::run_prepared_command_with_timeout(
        command,
        std::time::Duration::from_millis(500),
    );

    match output {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            extension_info_is_active(&stdout)
        }
        _ => false,
    }
}

fn extension_info_is_active(info: &str) -> bool {
    let enabled = info
        .lines()
        .filter_map(parse_extension_info_field)
        .any(|(key, value)| key == "Enabled" && value == "Yes");
    let active = info
        .lines()
        .filter_map(parse_extension_info_field)
        .any(|(key, value)| key == "State" && value == "ACTIVE");

    enabled && active
}

fn parse_extension_info_field(line: &str) -> Option<(&str, &str)> {
    let (key, value) = line.trim().split_once(':')?;
    Some((key.trim(), value.trim()))
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

    #[test]
    fn env_flag_value_parses_truthy_values() {
        assert!(env_flag_value_enabled("1"));
        assert!(env_flag_value_enabled("yes"));
        assert!(!env_flag_value_enabled("0"));
        assert!(!env_flag_value_enabled(""));
    }

    #[test]
    fn extension_info_active_requires_enabled_and_active_state() {
        let info = "\
bananatray@bananatray.github.io
  Name: BananaTray
  Enabled: Yes
  State: ACTIVE
";

        assert!(extension_info_is_active(info));
    }

    #[test]
    fn extension_info_out_of_date_is_not_active() {
        let info = "\
bananatray@bananatray.github.io
  Name: BananaTray
  Enabled: Yes
  State: OUT OF DATE
";

        assert!(!extension_info_is_active(info));
    }

    #[test]
    fn extension_info_disabled_is_not_active() {
        let info = "\
bananatray@bananatray.github.io
  Name: BananaTray
  Enabled: No
  State: ACTIVE
";

        assert!(!extension_info_is_active(info));
    }
}
