use crate::app_state::{NavigationState, ProviderStore, SettingsTab, SettingsUiState};
use crate::models::{AppSettings, ConnectionStatus, NavTab, ProviderKind};
use crate::refresh::{RefreshEvent, RefreshReason, RefreshRequest, RefreshResult};
use log::{debug, info, warn};
use smol::channel::Sender;
use std::sync::Arc;

use crate::notification::{send_system_notification, QuotaAlertTracker};

// ============================================================================
// 设置持久化（放在此处：紧密关联 AppSettings 操作，由调用方在修改后触发）
// ============================================================================

/// 将 AppSettings 持久化到磁盘（非阻塞，失败时仅记录日志）
pub(crate) fn persist_settings(settings: &AppSettings) {
    if let Err(err) = crate::settings_store::save(settings) {
        warn!(target: "settings", "failed to save settings: {err}");
    }
}

// ============================================================================
// 外部持久状态 (不随窗口销毁) — 纯组合容器
// ============================================================================

/// 应用持久状态，在窗口生命周期之外保持
pub struct AppState {
    pub provider_store: ProviderStore,
    pub nav: NavigationState,
    pub settings_ui: SettingsUiState,
    pub settings: AppSettings,
    /// 向 RefreshCoordinator 发送请求的通道
    pub refresh_tx: Sender<RefreshRequest>,
    /// 配额告警追踪器
    pub alert_tracker: QuotaAlertTracker,
    /// 当前 AppView 的弱引用，用于事件泵通知 UI 刷新
    pub view_entity: Option<gpui::WeakEntity<super::AppView>>,
}

impl AppState {
    pub fn new(refresh_tx: Sender<RefreshRequest>) -> Self {
        debug!(target: "app", "initializing AppState");
        let settings = crate::settings_store::load().unwrap_or_else(|err| {
            warn!(target: "settings", "failed to load saved settings: {err}");
            AppSettings::default()
        });
        crate::auto_launch::sync(settings.start_at_login);
        let manager = Arc::new(crate::providers::ProviderManager::new());
        let mut providers = manager.initial_statuses();
        for p in &mut providers {
            p.enabled = settings.is_provider_enabled(p.kind);
        }
        let first_enabled = ProviderKind::all()
            .iter()
            .find(|k| settings.is_provider_enabled(**k))
            .copied();

        let active_tab = if let Some(kind) = first_enabled {
            debug!(target: "app", "default active tab: Provider {:?}", kind);
            NavTab::Provider(kind)
        } else {
            debug!(target: "app", "default active tab: Settings (no providers enabled)");
            NavTab::Settings
        };

        Self {
            provider_store: ProviderStore { providers },
            nav: NavigationState {
                active_tab,
                last_provider_kind: first_enabled.unwrap_or(ProviderKind::Claude),
                generation: 0,
            },
            settings_ui: SettingsUiState {
                active_tab: SettingsTab::General,
                selected_provider: ProviderKind::Claude,
                cadence_dropdown_open: false,
                copilot_token_editing: false,
            },
            settings,
            refresh_tx,
            alert_tracker: QuotaAlertTracker::new(),
            view_entity: None,
        }
    }

    /// 向 RefreshCoordinator 发送请求（非阻塞）
    pub fn send_refresh(
        &self,
        request: RefreshRequest,
    ) -> Result<(), smol::channel::TrySendError<RefreshRequest>> {
        self.refresh_tx.try_send(request)
    }

    /// 选择新的刷新频率并同步到协调器
    pub fn select_cadence(&mut self, mins: Option<u64>) {
        self.settings.refresh_interval_mins = mins.unwrap_or(0);
        self.settings_ui.cadence_dropdown_open = false;
        self.sync_config_to_coordinator();
    }

    /// 通知协调器配置已变更
    pub fn sync_config_to_coordinator(&self) {
        let enabled: Vec<ProviderKind> = ProviderKind::all()
            .iter()
            .filter(|k| self.settings.is_provider_enabled(**k))
            .copied()
            .collect();
        let _ = self.send_refresh(RefreshRequest::UpdateConfig {
            interval_mins: self.settings.refresh_interval_mins,
            enabled,
        });
    }

    /// 统一处理来自 RefreshCoordinator 的事件，更新 Provider 状态
    /// 这是 **唯一** 修改 provider 连接状态的入口
    pub fn apply_refresh_event(&mut self, event: RefreshEvent) {
        match event {
            RefreshEvent::Started { kind } => {
                self.provider_store.mark_refreshing(kind);
            }
            RefreshEvent::Finished(outcome) => {
                let Some(p) = self.provider_store.find_mut(outcome.kind) else {
                    return;
                };
                match outcome.result {
                    RefreshResult::Success { data } => {
                        info!(target: "providers", "provider {:?} refresh succeeded: {} quotas", outcome.kind, data.quotas.len());
                        // 检测配额告警状态变化
                        let provider_name = p.display_name().to_string();
                        if let Some(alert) =
                            self.alert_tracker
                                .update(outcome.kind, &provider_name, &data.quotas)
                        {
                            if self.settings.session_quota_notifications {
                                let with_sound = self.settings.notification_sound;
                                send_system_notification(&alert, with_sound);
                            }
                        }
                        p.mark_refresh_succeeded(data);
                    }
                    RefreshResult::Unavailable { message } => {
                        debug!(target: "providers", "provider {:?} unavailable: {}", outcome.kind, message);
                        p.mark_unavailable(message);
                    }
                    RefreshResult::Failed { error, error_kind } => {
                        p.mark_refresh_failed(error, error_kind);
                    }
                    RefreshResult::SkippedCooldown
                    | RefreshResult::SkippedInFlight
                    | RefreshResult::SkippedDisabled => {}
                }
            }
        }
    }

    pub fn request_provider_refresh(&mut self, kind: ProviderKind, reason: RefreshReason) {
        if !self.settings.is_provider_enabled(kind) {
            debug!(target: "refresh", "ignoring refresh request for disabled provider {:?}", kind);
            return;
        }

        self.provider_store.mark_refreshing(kind);
        if let Err(err) = self.send_refresh(RefreshRequest::RefreshOne { kind, reason }) {
            warn!(target: "refresh", "failed to send refresh request: {}", err);
            if let Some(provider) = self.provider_store.find_mut(kind) {
                provider.connection = ConnectionStatus::Disconnected;
            }
        }
    }

    /// Toggle a provider on/off and update all related state.
    /// Returns updated settings.
    pub fn toggle_provider(&mut self, kind: ProviderKind) -> AppSettings {
        let new_val = !self.settings.is_provider_enabled(kind);
        info!(target: "providers", "toggling provider {:?} from {} to {}",
            kind, !new_val, new_val);
        self.settings.set_provider_enabled(kind, new_val);

        if let Some(p) = self.provider_store.find_mut(kind) {
            p.enabled = new_val;
        }

        if new_val {
            self.nav.switch_to(NavTab::Provider(kind));
        } else {
            self.nav.fallback_on_disable(kind, &self.settings);
        }

        // 通知协调器配置变更，并请求刷新
        self.sync_config_to_coordinator();
        if new_val {
            self.request_provider_refresh(kind, RefreshReason::ProviderToggled);
        }

        self.settings.clone()
    }

    /// 获取当前活跃 provider 的状态徽章文案
    pub fn header_status_text(&self) -> (String, crate::app_state::HeaderStatusKind) {
        crate::app_state::compute_header_status(&self.nav, &self.provider_store)
    }

    /// 根据活跃 Provider 的 quota 数量动态计算弹出窗口高度
    pub fn popup_height(&self) -> f32 {
        let kind = if let NavTab::Provider(k) = self.nav.active_tab {
            k
        } else {
            self.nav.last_provider_kind
        };
        let provider = self.provider_store.find(kind);
        let quota_count = provider.map(|p| p.quotas.len()).unwrap_or(1);
        let has_dashboard = self.settings.show_dashboard_button
            && provider
                .map(|p| !p.dashboard_url().is_empty())
                .unwrap_or(false);

        crate::models::compute_popup_height_detailed(quota_count, has_dashboard)
    }
}
