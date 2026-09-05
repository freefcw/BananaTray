use crate::application::AppSession;
use crate::models::AppSettings;
use crate::providers::ProviderManagerHandle;
use crate::refresh::{RefreshRequest, RefreshWorker};
use log::debug;
use std::path::PathBuf;

use super::SettingsWriter;
use super::{
    BackgroundJobSender, CustomProviderJob, CustomProviderResults, PersistentJobSender,
    ScriptTestJob,
};

// ============================================================================
// 外部持久状态 (不随窗口销毁) — 纯组合容器
// ============================================================================

/// 应用持久状态，在窗口生命周期之外保持
pub struct AppState {
    pub session: AppSession,
    pub manager: ProviderManagerHandle,
    /// 向 RefreshCoordinator 发送请求的通道
    refresh_worker: RefreshWorker,
    /// 向专用阻塞线程发送 NewAPI / Script Provider CRUD 文件 I/O。
    pub(crate) custom_provider_tx: PersistentJobSender<CustomProviderJob>,
    /// worker 已完成但前台 reducer 尚未结算的持久事务结果。
    pub(crate) custom_provider_results: CustomProviderResults,
    /// 向独立阻塞线程发送脚本 Run Test，避免长 timeout 阻塞 CRUD。
    pub(crate) script_test_tx: BackgroundJobSender<ScriptTestJob>,
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
        custom_provider_tx: PersistentJobSender<CustomProviderJob>,
        script_test_tx: BackgroundJobSender<ScriptTestJob>,
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
            custom_provider_tx,
            custom_provider_results: CustomProviderResults::default(),
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

    #[cfg(test)]
    pub(crate) fn shutdown_background_workers_before(
        &mut self,
        deadline: std::time::Instant,
    ) -> bool {
        self.custom_provider_tx.close();
        self.script_test_tx.request_shutdown();
        self.custom_provider_tx.join_before(deadline) & self.script_test_tx.join_before(deadline)
    }

    /// 先有界等待后台工作，再结算已收到的事务结果，最后完成 settings 快照落盘。
    ///
    /// refresh / script-test / custom-provider worker 共享退出 deadline；CRUD 超时 detach
    /// 不保证未完成事务结算，settings writer 随后持久化已结算的领域状态。
    pub(crate) fn shutdown_before(&mut self, deadline: std::time::Instant) {
        self.refresh_worker.request_shutdown();
        self.custom_provider_tx.close();
        self.script_test_tx.request_shutdown();

        let _ = self.refresh_worker.join_before(deadline);
        let _ = self.script_test_tx.join_before(deadline);
        let custom_provider_stopped = self.custom_provider_tx.join_before(deadline);
        if !custom_provider_stopped {
            log::warn!(
                target: "settings",
                "custom-provider I/O did not stop before quit deadline; pending transaction detached"
            );
        }
        for action in self.custom_provider_results.drain() {
            // 退出阶段只结算领域状态；通知、render、reload 等非持久 effect 无需执行。
            let _ = crate::application::reduce(&mut self.session, action);
        }
        // 将所有事务完成动作归并后的权威状态作为 writer 的最后一份快照。
        self.settings_writer.schedule(self.session.settings.clone());
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
