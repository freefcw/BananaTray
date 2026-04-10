#![allow(dead_code, unused_imports)]
//! 平台适配层
//!
//! 集中管理所有平台相关的代码：
//! - `assets` — GPUI 资源加载（多平台路径解析）
//! - `auto_launch` — 开机自启动（macOS SMAppService / Linux XDG autostart）
//! - `logging` — 日志系统初始化（fern + panic hook）
//! - `notification` — 系统通知 + 配额预警状态机
//! - `single_instance` — 单实例检测（IPC local socket）
//! - `system` — 系统工具（打开 URL、剪贴板、暗色模式检测、系统信息）

// --- GPUI 依赖模块 ---
// assets 使用 GPUI AssetSource trait，single_instance 仅在运行时使用
// 测试时（--no-default-features）不编译这些模块
#[cfg(feature = "app")]
pub(crate) mod assets;
#[cfg(feature = "app")]
pub(crate) mod single_instance;

#[cfg(feature = "app")]
pub(crate) use assets::Assets;

// --- 纯逻辑 / 平台原生模块 ---
// 被 app_state、application 等 GPUI-free 模块引用，必须始终可编译
pub mod auto_launch;
pub(crate) mod logging;
pub mod notification;
pub mod system;
