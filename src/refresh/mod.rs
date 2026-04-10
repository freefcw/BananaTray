//! 刷新模块 — 后台周期性刷新 Provider 数据的协调器。
//!
//! ## 模块结构
//! - `types` — 消息类型（Request / Event / Result）
//! - `scheduler` — 纯调度决策引擎（cooldown / eligibility / deadline）
//! - `coordinator` — 事件循环 + 并发执行

mod coordinator;
mod scheduler;
mod types;

pub use coordinator::RefreshCoordinator;
pub use types::*;
