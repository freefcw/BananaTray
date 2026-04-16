//! 托盘功能聚合模块
//!
//! 包含系统托盘的所有相关功能：
//! - `controller` — 弹窗窗口生命周期管理（TrayController）
//! - `icon` — 托盘图标样式管理（使用 GPUI 原生 set_tray_icon_rendering_mode API）

pub(crate) mod controller;
pub(crate) mod icon;

#[allow(unused_imports)]
pub(crate) use controller::TrayController;
pub use icon::apply_tray_icon;
