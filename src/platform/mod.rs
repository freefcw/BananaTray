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

// ============================================================================
// 应用标识常量（单一来源）
// ============================================================================

/// 应用显示名称（macOS 路径、通知、桌面条目等）
pub const APP_NAME: &str = "BananaTray";
/// 应用小写 ID（Linux 路径、日志目录、socket 名称等）
pub const APP_ID_LOWER: &str = "bananatray";
/// 应用 Bundle ID（macOS plist、Linux desktop entry ID）
#[cfg(target_os = "linux")]
pub const APP_BUNDLE_ID: &str = "com.bananatray.app";

// --- GPUI 依赖模块 ---
// assets 使用 GPUI AssetSource trait；single_instance / auto_launch /
// notification 只在桌面 app 运行时使用。关闭 `app` feature 时这些模块不编译，
// 这样 lib-only 校验不会引入托盘壳和平台通知依赖。
#[cfg(feature = "app")]
pub(crate) mod assets;
#[cfg(feature = "app")]
pub mod auto_launch;
#[cfg(feature = "app")]
pub mod notification;
#[cfg(feature = "app")]
pub(crate) mod single_instance;

// --- 始终编译的平台模块 ---
// 供 bootstrap/runtime 和无 UI 场景复用；不承载 application 业务状态机
pub(crate) mod logging;
pub mod paths;
pub mod system;
