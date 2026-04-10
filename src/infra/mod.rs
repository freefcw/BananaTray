#![allow(dead_code, unused_imports)]
//! 应用基础设施模块
//!
//! 包含应用启动和运行所需的底层基础设施：
//! - `assets` — GPUI 资源加载（多平台路径解析）
//! - `logging` — 日志系统初始化（fern + panic hook）
//! - `single_instance` — 单实例检测（IPC local socket）

pub(crate) mod assets;
pub(crate) mod logging;
pub(crate) mod single_instance;

pub(crate) use assets::Assets;
