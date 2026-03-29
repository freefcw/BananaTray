//! BananaTray - 系统托盘配额监控应用
//!
//! 注意：这是一个 bin + lib 混合 crate，lib 部分主要供测试使用。

pub mod models;
pub mod providers;
pub mod refresh;
pub mod settings_store;
pub mod theme;
pub mod utils;

// app 模块包含 GPUI 代码，测试时可能触发编译器 bug
// 因此默认不导出，需要的话可以通过 feature 启用
#[cfg(feature = "app")]
pub mod app;
#[cfg(feature = "app")]
pub mod logging;
