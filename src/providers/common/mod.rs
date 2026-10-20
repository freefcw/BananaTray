pub(crate) mod cli;
pub(crate) mod config_paths;
pub(crate) mod http_client;
pub(crate) mod jwt;
pub(crate) mod path_resolver;
pub(crate) mod runner;
pub(crate) mod secret;

/// 单条外部数据边界的内存上限（HTTP 正文 / CLI 流 / PTY 捕获共用）。
///
/// 刷新线程与托盘是同一个进程：任何一条狂吐输出的后端（HTTP 响应、
/// CLI stdout/stderr、PTY 会话）都不能在超时窗口内把进程内存打满。
/// 三条边界共享同一条防御策略，所以上限定义在一处。
pub(crate) const MAX_EXTERNAL_PAYLOAD_BYTES: u64 = 8 * 1024 * 1024;
