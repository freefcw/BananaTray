use crate::application::AppSession;
use crate::models::AppSettings;
use crate::providers::ProviderManager;
use crate::refresh::RefreshRequest;
use log::{debug, warn};
use smol::channel::Sender;
use std::path::PathBuf;

// ============================================================================
// 设置持久化（放在此处：紧密关联 AppSettings 操作，由调用方在修改后触发）
// ============================================================================

/// 将 AppSettings 持久化到磁盘。
///
/// 返回 `true` 表示成功，`false` 表示失败（已记录日志）。
/// 大多数调用点可忽略返回值（fire-and-forget），仅在需要区分
/// 成功/失败并给用户不同反馈时才检查（如 SaveNewApiProvider）。
pub(crate) fn persist_settings(settings: &AppSettings) -> bool {
    match crate::settings_store::save(settings) {
        Ok(_) => true,
        Err(err) => {
            warn!(target: "settings", "failed to save settings: {err}");
            false
        }
    }
}

// ============================================================================
// 外部持久状态 (不随窗口销毁) — 纯组合容器
// ============================================================================

/// 应用持久状态，在窗口生命周期之外保持
pub struct AppState {
    pub session: AppSession,
    /// 向 RefreshCoordinator 发送请求的通道
    pub refresh_tx: Sender<RefreshRequest>,
    /// 当前 AppView 的弱引用，用于事件泵通知 UI 刷新
    pub view_entity: Option<gpui::WeakEntity<super::AppView>>,
    /// 日志文件路径（Debug Tab 展示用）
    pub log_path: Option<PathBuf>,
}

impl AppState {
    pub fn new(
        refresh_tx: Sender<RefreshRequest>,
        manager: &ProviderManager,
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
            refresh_tx,
            view_entity: None,
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
