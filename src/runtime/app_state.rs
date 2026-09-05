use crate::application::AppSession;
use crate::models::{AppSettings, ScriptProviderConfig};
use crate::providers::ProviderManagerHandle;
use crate::refresh::{RefreshRequest, RefreshWorker};
use log::debug;
use smol::channel::Sender;
use std::path::PathBuf;

use super::SettingsWriter;

// ============================================================================
// 外部持久状态 (不随窗口销毁) — 纯组合容器
// ============================================================================

/// 应用持久状态，在窗口生命周期之外保持
pub struct AppState {
    pub session: AppSession,
    pub manager: ProviderManagerHandle,
    /// 向 RefreshCoordinator 发送请求的通道
    refresh_worker: RefreshWorker,
    /// 向前台事件泵发送脚本测试请求。
    pub(crate) script_test_tx: Sender<(u64, ScriptProviderConfig)>,
    /// 设置文件 debounce 写入器（所有持久化统一通过此句柄串行化）
    pub(crate) settings_writer: SettingsWriter,
    /// 日志文件路径（Debug Tab 展示用）
    pub log_path: Option<PathBuf>,
    #[cfg(target_os = "linux")]
    linux_popup_auto_hide_suppressed_until: Option<std::time::Instant>,
    #[cfg(target_os = "linux")]
    linux_popup_position_save_requested: bool,
}

impl AppState {
    pub(crate) fn new(
        refresh_worker: RefreshWorker,
        script_test_tx: Sender<(u64, ScriptProviderConfig)>,
        manager: ProviderManagerHandle,
        settings: AppSettings,
        log_path: Option<PathBuf>,
    ) -> Self {
        debug!(target: "app", "initializing AppState");
        let providers = manager.snapshot().initial_statuses();
        let session = AppSession::new(settings, providers);
        debug!(
            target: "app",
            "default active tab: {:?}",
            session.nav.active_tab
        );

        Self {
            session,
            manager,
            refresh_worker,
            script_test_tx,
            settings_writer: SettingsWriter::spawn(),
            log_path,
            #[cfg(target_os = "linux")]
            linux_popup_auto_hide_suppressed_until: None,
            #[cfg(target_os = "linux")]
            linux_popup_position_save_requested: false,
        }
    }

    /// 向 RefreshCoordinator 发送请求（非阻塞）。
    ///
    /// 通道为 unbounded：失败仅发生在协调器线程终止（channel 关闭）后。
    pub fn send_refresh(
        &self,
        request: RefreshRequest,
    ) -> Result<(), smol::channel::TrySendError<RefreshRequest>> {
        self.refresh_worker.try_send(request)
    }

    /// 关闭设置写入器，并等待最后一份待写设置完成持久化。
    #[cfg(test)]
    pub(crate) fn shutdown_settings_writer(&mut self) {
        self.settings_writer.shutdown_and_join();
    }

    /// 在退出截止时间内回收刷新线程，随后完成设置最终写入。
    pub(crate) fn shutdown_before(&mut self, deadline: std::time::Instant) {
        self.refresh_worker.request_shutdown();
        let _ = self.refresh_worker.join_before(deadline);
        self.settings_writer.shutdown_and_join();
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn begin_linux_popup_drag(&mut self, duration: std::time::Duration) {
        self.suppress_linux_popup_auto_hide_for(duration);
        self.linux_popup_position_save_requested = true;
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn suppress_linux_popup_auto_hide_for(&mut self, duration: std::time::Duration) {
        self.linux_popup_auto_hide_suppressed_until = Some(std::time::Instant::now() + duration);
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn linux_popup_auto_hide_suppression_remaining(
        &self,
    ) -> Option<std::time::Duration> {
        self.linux_popup_auto_hide_suppressed_until
            .and_then(|until| until.checked_duration_since(std::time::Instant::now()))
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn should_save_linux_popup_position(&self) -> bool {
        self.linux_popup_position_save_requested
    }
}
