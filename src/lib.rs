#![recursion_limit = "512"]

//! BananaTray - 系统托盘配额监控应用
//!
//! 注意：这是一个 bin + lib 混合 crate，lib 部分主要供测试使用。

rust_i18n::i18n!("locales", fallback = "en");

pub mod application;
pub mod i18n;
pub mod models;
pub mod platform;
pub mod providers;
pub mod refresh;
pub mod settings_store;
#[cfg(feature = "app")]
pub mod theme;
#[cfg(all(test, feature = "app"))]
mod theme_tests;
pub mod utils;

// GPUI 视图层和运行时模块，测试时不编译
#[cfg(feature = "app")]
pub mod runtime;
#[cfg(feature = "app")]
pub mod tray;
#[cfg(feature = "app")]
pub mod ui;
