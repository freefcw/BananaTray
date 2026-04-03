//! 全局日志捕获器 — 在调试刷新期间将日志条目缓存到内存 ring buffer
//!
//! 设计要点：
//! - 线程安全（Arc<Mutex> + AtomicBool），可被任何线程的 log 调用写入
//! - 通过 `enabled` 原子标志控制是否录入，正常运行不捕获
//! - Ring buffer（VecDeque），最多 MAX_ENTRIES 条，超出丢弃最旧
//! - target 过滤：只捕获 providers*、refresh、interactive_runner 相关的日志

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

/// 单条日志条目
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: log::Level,
    pub target: String,
    pub message: String,
}

impl LogEntry {
    /// 格式化为单行文本（用于 Copy Logs）
    #[allow(dead_code)]
    pub fn format_line(&self) -> String {
        format!(
            "{} [{}] {} {}",
            self.timestamp, self.level, self.target, self.message
        )
    }
}

/// Ring buffer 容量
const MAX_ENTRIES: usize = 500;

/// 需要捕获的 log target 前缀
const CAPTURED_TARGETS: &[&str] = &["providers", "refresh", "interactive_runner"];

/// 全局单例
static INSTANCE: OnceLock<LogCapture> = OnceLock::new();

pub struct LogCapture {
    buffer: Mutex<VecDeque<LogEntry>>,
    enabled: AtomicBool,
}

impl LogCapture {
    fn new() -> Self {
        Self {
            buffer: Mutex::new(VecDeque::with_capacity(MAX_ENTRIES)),
            enabled: AtomicBool::new(false),
        }
    }

    /// 获取全局单例
    pub fn global() -> &'static LogCapture {
        INSTANCE.get_or_init(LogCapture::new)
    }

    /// 启用日志捕获
    pub fn enable(&self) {
        self.enabled.store(true, Ordering::SeqCst);
    }

    /// 停用日志捕获
    pub fn disable(&self) {
        self.enabled.store(false, Ordering::SeqCst);
    }

    /// 当前是否启用
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }

    /// 清空缓冲区
    pub fn clear(&self) {
        if let Ok(mut buf) = self.buffer.lock() {
            buf.clear();
        }
    }

    /// 尝试推入一条日志（由 logging.rs 的 format 闭包调用）
    ///
    /// 如果未启用或 target 不匹配，直接跳过（零开销）
    pub fn try_push(&self, level: log::Level, target: &str, message: &str) {
        if !self.is_enabled() {
            return;
        }
        if !should_capture(target) {
            return;
        }

        let entry = LogEntry {
            timestamp: chrono::Local::now().format("%H:%M:%S%.3f").to_string(),
            level,
            target: target.to_string(),
            message: message.to_string(),
        };

        if let Ok(mut buf) = self.buffer.lock() {
            if buf.len() >= MAX_ENTRIES {
                buf.pop_front();
            }
            buf.push_back(entry);
        }
    }

    /// 读取所有已捕获的日志条目（克隆）
    pub fn entries(&self) -> Vec<LogEntry> {
        self.buffer
            .lock()
            .map(|buf| buf.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// 将所有条目格式化为纯文本（用于 Copy Logs）
    #[allow(dead_code)]
    pub fn format_all(&self) -> String {
        self.entries()
            .iter()
            .map(|e| e.format_line())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// 检查 target 是否应该被捕获
fn should_capture(target: &str) -> bool {
    CAPTURED_TARGETS
        .iter()
        .any(|prefix| target.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 每个测试用独立的 LogCapture 实例，不走全局单例
    fn make_capture() -> LogCapture {
        LogCapture::new()
    }

    #[test]
    fn disabled_by_default() {
        let cap = make_capture();
        assert!(!cap.is_enabled());
        assert!(cap.entries().is_empty());
    }

    #[test]
    fn enable_disable_toggle() {
        let cap = make_capture();
        cap.enable();
        assert!(cap.is_enabled());
        cap.disable();
        assert!(!cap.is_enabled());
    }

    #[test]
    fn push_when_disabled_is_noop() {
        let cap = make_capture();
        cap.try_push(log::Level::Info, "providers", "hello");
        assert!(cap.entries().is_empty());
    }

    #[test]
    fn push_when_enabled_captures() {
        let cap = make_capture();
        cap.enable();
        cap.try_push(log::Level::Info, "providers", "test message");
        let entries = cap.entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].message, "test message");
        assert_eq!(entries[0].level, log::Level::Info);
    }

    #[test]
    fn filters_irrelevant_targets() {
        let cap = make_capture();
        cap.enable();
        cap.try_push(log::Level::Info, "app", "should be ignored");
        cap.try_push(log::Level::Info, "tray", "should be ignored");
        cap.try_push(log::Level::Info, "settings", "should be ignored");
        assert!(cap.entries().is_empty());
    }

    #[test]
    fn captures_relevant_targets() {
        let cap = make_capture();
        cap.enable();
        cap.try_push(log::Level::Debug, "providers", "hit");
        cap.try_push(log::Level::Info, "providers::kiro", "hit");
        cap.try_push(log::Level::Warn, "refresh", "hit");
        cap.try_push(log::Level::Info, "interactive_runner", "hit");
        assert_eq!(cap.entries().len(), 4);
    }

    #[test]
    fn ring_buffer_evicts_oldest() {
        let cap = make_capture();
        cap.enable();
        for i in 0..MAX_ENTRIES + 10 {
            cap.try_push(log::Level::Info, "providers", &format!("msg {}", i));
        }
        let entries = cap.entries();
        assert_eq!(entries.len(), MAX_ENTRIES);
        // 最旧的 10 条被丢弃，第一条应该是 "msg 10"
        assert_eq!(entries[0].message, "msg 10");
    }

    #[test]
    fn clear_empties_buffer() {
        let cap = make_capture();
        cap.enable();
        cap.try_push(log::Level::Info, "providers", "msg");
        assert_eq!(cap.entries().len(), 1);
        cap.clear();
        assert!(cap.entries().is_empty());
    }

    #[test]
    fn format_all_produces_text() {
        let cap = make_capture();
        cap.enable();
        cap.try_push(log::Level::Info, "refresh", "started");
        cap.try_push(log::Level::Debug, "providers", "checking");
        let text = cap.format_all();
        assert!(text.contains("refresh"));
        assert!(text.contains("providers"));
        assert!(text.contains("started"));
        assert!(text.contains("checking"));
    }

    #[test]
    fn should_capture_matches_prefixes() {
        assert!(should_capture("providers"));
        assert!(should_capture("providers::kiro"));
        assert!(should_capture("providers::gemini::auth"));
        assert!(should_capture("refresh"));
        assert!(should_capture("interactive_runner"));
        assert!(!should_capture("app"));
        assert!(!should_capture("tray"));
        assert!(!should_capture("settings"));
    }
}
