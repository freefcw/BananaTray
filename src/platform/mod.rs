#![allow(dead_code, unused_imports)]
//! 平台适配层
//!
//! 集中管理所有平台相关的代码：
//! - `assets` — GPUI 资源加载（多平台路径解析）
//! - `paths` — 配置目录与自定义 Provider 路径解析
//! - `auto_launch` — 开机自启动（macOS SMAppService / Linux XDG autostart）
//! - `logging` — 日志系统初始化（fern + panic hook）
//! - `notification` — 系统通知发送（OS adapter）
//! - `single_instance` — 单实例检测（IPC local socket）
//! - `system` — 系统工具（打开 URL、剪贴板、暗色模式检测、系统信息）

// --- GPUI 依赖模块 ---
// assets 使用 GPUI AssetSource trait，single_instance 仅在运行时使用
// 关闭 `app` feature 时这些模块不编译
#[cfg(feature = "app")]
pub(crate) mod assets;
#[cfg(feature = "app")]
pub(crate) mod single_instance;

#[cfg(feature = "app")]
pub(crate) use assets::Assets;

// --- 始终编译的平台模块 ---
// 供 bootstrap/runtime 和无 UI 场景复用；不承载 application 业务状态机
pub mod auto_launch;
pub(crate) mod logging;
pub mod notification;
pub mod paths;
pub mod system;
