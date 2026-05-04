use crate::application::AppSession;
use crate::models::AppSettings;
use crate::providers::ProviderManagerHandle;
use crate::refresh::RefreshRequest;
use log::debug;
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
    pub refresh_tx: smol::channel::Sender<RefreshRequest>,
    /// 设置文件 debounce 写入器（所有持久化统一通过此句柄串行化）
    pub(crate) settings_writer: SettingsWriter,
    /// 日志文件路径（Debug Tab 展示用）
    pub log_path: Option<PathBuf>,
    #[cfg(target_os = "linux")]
    linux_popup_auto_hide_suppressed_until: Option<std::time::Instant>,
    #[cfg(target_os = "linux")]
    linux_popup_position_save_requested: bool,
    /// D-Bus 服务句柄（Linux: 供事件泵发射信号给 GNOME Shell Extension）
    #[cfg(target_os = "linux")]
    #[allow(dead_code)]
    pub(crate) dbus_handle: Option<crate::dbus::DBusServiceHandle>,
}

impl AppState {
    pub fn new(
        refresh_tx: smol::channel::Sender<RefreshRequest>,
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
            refresh_tx,
            settings_writer: SettingsWriter::spawn(),
            log_path,
            #[cfg(target_os = "linux")]
            linux_popup_auto_hide_suppressed_until: None,
            #[cfg(target_os = "linux")]
            linux_popup_position_save_requested: false,
            #[cfg(target_os = "linux")]
            dbus_handle: None,
        }
    }

    /// 向 RefreshCoordinator 发送请求（非阻塞）
    pub fn send_refresh(
        &self,
        request: RefreshRequest,
    ) -> Result<(), smol::channel::TrySendError<RefreshRequest>> {
        self.refresh_tx.try_send(request)
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
