use crate::application::AppSession;
use crate::models::AppSettings;
use crate::providers::ProviderManager;
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
    pub manager: std::sync::Arc<ProviderManager>,
    /// 向 RefreshCoordinator 发送请求的通道
    pub refresh_tx: smol::channel::Sender<RefreshRequest>,
    /// 设置文件 debounce 写入器（所有持久化统一通过此句柄串行化）
    pub(crate) settings_writer: SettingsWriter,
    /// 日志文件路径（Debug Tab 展示用）
    pub log_path: Option<PathBuf>,
}

impl AppState {
    pub fn new(
        refresh_tx: smol::channel::Sender<RefreshRequest>,
        manager: std::sync::Arc<ProviderManager>,
        settings: AppSettings,
        log_path: Option<PathBuf>,
    ) -> Self {
        debug!(target: "app", "initializing AppState");
        let providers = manager.initial_statuses();
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
        }
    }

    /// 向 RefreshCoordinator 发送请求（非阻塞）
    pub fn send_refresh(
        &self,
        request: RefreshRequest,
    ) -> Result<(), smol::channel::TrySendError<RefreshRequest>> {
        self.refresh_tx.try_send(request)
    }
}
