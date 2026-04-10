//! 托盘功能聚合模块
//!
//! 包含系统托盘的所有相关功能：
//! - `controller` — 弹窗窗口生命周期管理（TrayController）
//! - `display` — macOS 多显示器感知的弹窗定位（CoreGraphics FFI）
//! - `icon` — 托盘图标样式管理（含 macOS setTemplate hack）

pub(crate) mod controller;
#[cfg(target_os = "macos")]
pub(crate) mod display;
pub(crate) mod icon;

#[allow(unused_imports)]
pub(crate) use controller::TrayController;
pub use icon::apply_tray_icon;
