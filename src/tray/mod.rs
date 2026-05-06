//! 托盘功能聚合模块
//!
//! 包含系统托盘的所有相关功能：
//! - `activation` — popup 失焦/auto-hide 状态机
//! - `command` — 托盘入口命令策略
//! - `controller` — 弹窗窗口生命周期管理（TrayController）
//! - `icon` — 托盘图标样式管理（使用 GPUI 原生 set_tray_icon_rendering_mode API）
//! - `lifecycle` — 关闭清理与窗口 slot 幂等 helper
//! - `positioning` — 弹窗定位策略
//! - `linux_popup` — Linux 特有的隐藏复用、拖动位置保存和 auto-hide 复查

mod activation;
mod command;
pub(crate) mod controller;
pub(crate) mod icon;
mod lifecycle;
#[cfg(target_os = "linux")]
mod linux_popup;
mod observers;
mod positioning;

#[allow(unused_imports)]
pub(crate) use controller::TrayController;
pub use icon::apply_tray_icon;

/// 当前进程是否应该注册 GPUI 传统托盘入口。
///
/// GNOME Shell Extension ACTIVE 时，面板入口完全由扩展负责；Rust 侧一旦调用
/// GPUI tray API，Linux KSNI 后端就会创建一个空 StatusNotifierItem。
#[cfg(target_os = "linux")]
pub(crate) fn should_use_gpui_tray() -> bool {
    !crate::platform::gnome_detect::should_use_gnome_extension()
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn should_use_gpui_tray() -> bool {
    true
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    #[test]
    fn gpui_tray_is_disabled_when_gnome_extension_mode_is_forced() {
        std::env::set_var("BANANATRAY_FORCE_GNOME_EXTENSION", "1");
        assert!(!super::should_use_gpui_tray());
        std::env::remove_var("BANANATRAY_FORCE_GNOME_EXTENSION");
    }
}
