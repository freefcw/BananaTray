mod nav;
mod provider_logic;
mod provider_panel;
pub(crate) mod settings_window;
mod tray_settings;
mod widgets;

pub use settings_window::schedule_open_settings_window;
pub(crate) use widgets::with_multiline_tooltip;

use crate::app_state::{NavigationState, ProviderStore, SettingsTab, SettingsUiState};
use crate::models::{AppSettings, AppTheme, ConnectionStatus, NavTab, ProviderKind};
use crate::refresh::{RefreshEvent, RefreshReason, RefreshRequest, RefreshResult};
use crate::theme::Theme;
use gpui::*;
use log::{debug, info, warn};
use rust_i18n::t;
use smol::channel::Sender;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use crate::notification::{send_system_notification, QuotaAlertTracker};

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
    pub view_entity: Option<gpui::WeakEntity<AppView>>,
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
    /// < 1m: "● Synced", 1~59m: "● Xm ago", ≥ 1h: "● Xh ago"
    /// Refreshing: "● Refreshing", 无数据: "● Offline"
    pub fn header_status_text(&self) -> (String, HeaderStatusKind) {
        let kind = match self.nav.active_tab {
            NavTab::Provider(k) => k,
            NavTab::Settings => self.nav.last_provider_kind,
        };
        let Some(provider) = self.provider_store.find(kind) else {
            return ("Offline".to_string(), HeaderStatusKind::Offline);
        };

        if provider.connection == ConnectionStatus::Refreshing {
            return ("Syncing…".to_string(), HeaderStatusKind::Syncing);
        }

        if let Some(instant) = provider.last_refreshed_instant {
            let secs = instant.elapsed().as_secs();
            if secs < 60 {
                ("Synced".to_string(), HeaderStatusKind::Synced)
            } else if secs < 3600 {
                (format!("{}m ago", secs / 60), HeaderStatusKind::Stale)
            } else {
                (format!("{}h ago", secs / 3600), HeaderStatusKind::Stale)
            }
        } else {
            match provider.connection {
                ConnectionStatus::Error => ("Error".to_string(), HeaderStatusKind::Offline),
                ConnectionStatus::Disconnected => {
                    ("Offline".to_string(), HeaderStatusKind::Offline)
                }
                _ => ("Waiting".to_string(), HeaderStatusKind::Syncing),
            }
        }
    }
}

/// 头部状态徽章类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderStatusKind {
    Synced,
    Syncing,
    Stale,
    Offline,
}

// ============================================================================
// 弹出窗口高度计算（布局常量在 models::PopupLayout 中）
// ============================================================================

pub(crate) use crate::models::PopupLayout;

/// 根据活跃 Provider 的 quota 数量动态计算弹出窗口高度
pub(crate) fn compute_popup_height(state: &AppState) -> f32 {
    let kind = if let NavTab::Provider(k) = state.nav.active_tab {
        k
    } else {
        state.nav.last_provider_kind
    };
    let provider = state.provider_store.find(kind);
    let quota_count = provider.map(|p| p.quotas.len()).unwrap_or(1);
    let has_dashboard = provider
        .map(|p| !p.dashboard_url().is_empty())
        .unwrap_or(false);

    crate::models::compute_popup_height_detailed(quota_count, has_dashboard)
}

pub(crate) fn persist_settings(settings: &AppSettings) {
    if let Err(err) = crate::settings_store::save(settings) {
        warn!(target: "settings", "failed to save settings: {err}");
    }
}

// ============================================================================
// 窗口视图 (可多次创建/销毁)
// ============================================================================

pub struct AppView {
    pub(crate) state: Rc<RefCell<AppState>>,
    pub(crate) _activation_sub: Option<gpui::Subscription>,
    pub(crate) nav_scroll_handle: gpui::ScrollHandle,
}

impl AppView {
    pub fn new(state: Rc<RefCell<AppState>>, cx: &mut Context<Self>) -> Self {
        let theme = match state.borrow().settings.theme.resolve() {
            AppTheme::Light => Theme::light(),
            AppTheme::Dark => Theme::dark(),
            AppTheme::System => unreachable!("resolve() never returns System"),
        };
        cx.set_global(theme);

        state.borrow_mut().view_entity = Some(cx.entity().downgrade());

        Self {
            state,
            _activation_sub: None,
            nav_scroll_handle: gpui::ScrollHandle::new(),
        }
    }

    // ========================================================================
    // 头部区域：应用名 + 连接状态徽章
    // ========================================================================

    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<Theme>();
        let state = self.state.borrow();
        let (status_text, status_kind) = state.header_status_text();
        drop(state);

        let (dot_color, badge_bg) = match status_kind {
            HeaderStatusKind::Synced => (theme.badge_healthy, theme.badge_synced_bg),
            HeaderStatusKind::Syncing => (theme.text_accent, rgba(0x3b82f61a).into()),
            HeaderStatusKind::Stale => (theme.text_muted, theme.bg_subtle),
            HeaderStatusKind::Offline => (theme.badge_offline, theme.btn_danger_bg),
        };

        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .px(px(16.0))
            .py(px(12.0))
            .border_b_1()
            .border_color(theme.border_subtle)
            .child(
                // 左侧：应用图标 + 名称
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    // 应用图标
                    .child(
                        div()
                            .w(px(36.0))
                            .h(px(36.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(10.0))
                            .bg(theme.bg_subtle)
                            .border_1()
                            .border_color(theme.border_subtle)
                            .child(widgets::render_svg_icon(
                                "src/icons/tray_icon.svg",
                                px(20.0),
                                theme.text_accent,
                            )),
                    )
                    // 应用名称
                    .child(
                        div()
                            .flex_col()
                            .gap(px(1.0))
                            .child(
                                div()
                                    .text_size(px(15.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(theme.text_primary)
                                    .child("BananaTray"),
                            )
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.text_muted)
                                    .child("AI USAGE MONITOR"),
                            ),
                    ),
            )
            .child(
                // 右侧：状态徽章
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .px(px(10.0))
                    .py(px(5.0))
                    .rounded(px(12.0))
                    .bg(badge_bg)
                    .child(div().w(px(6.0)).h(px(6.0)).rounded_full().bg(dot_color))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_secondary)
                            .child(status_text),
                    ),
            )
    }

    // ========================================================================
    // 底部工具栏：Sync Data + Settings + Close
    // ========================================================================

    fn render_global_actions(
        &self,
        active_tab: NavTab,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.global::<Theme>();
        let border_color = theme.border_subtle;

        // Sync Data 按钮（触发当前 provider 的刷新）
        let sync_btn = {
            let is_provider = matches!(active_tab, NavTab::Provider(_));
            let entity = cx.entity().clone();
            let kind = match active_tab {
                NavTab::Provider(k) => Some(k),
                _ => None,
            };
            let theme = cx.global::<Theme>();

            let is_refreshing = kind
                .and_then(|k| {
                    self.state
                        .borrow()
                        .provider_store
                        .find(k)
                        .map(|p| p.connection == ConnectionStatus::Refreshing)
                })
                .unwrap_or(false);

            let label = if is_refreshing {
                t!("provider.status.refreshing").to_string()
            } else {
                t!("tooltip.refresh").to_string()
            };

            let mut btn = div()
                .flex()
                .items_center()
                .justify_center()
                .gap(px(6.0))
                .px(px(20.0))
                .py(px(10.0))
                .rounded(px(10.0))
                .bg(theme.btn_sync_bg)
                .border_1()
                .border_color(theme.btn_sync_bg)
                .cursor_pointer()
                .hover(|style| style.opacity(0.8))
                .child(widgets::render_svg_icon(
                    "src/icons/refresh.svg",
                    px(14.0),
                    theme.btn_sync_text,
                ))
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.btn_sync_text)
                        .child(label),
                );

            if is_provider && !is_refreshing {
                btn = btn.on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    if let Some(k) = kind {
                        entity.update(cx, |view, cx| {
                            view.state
                                .borrow_mut()
                                .request_provider_refresh(k, RefreshReason::Manual);
                            cx.notify();
                        });
                    }
                });
            }

            btn
        };

        // 设置按钮（圆形）
        let settings_btn = self.render_circle_button(
            "src/icons/settings.svg",
            cx.global::<Theme>().text_secondary,
            cx.global::<Theme>().bg_subtle,
            cx.global::<Theme>().border_subtle,
        );
        let state_for_settings = self.state.clone();
        let settings_btn = settings_btn.on_mouse_down(MouseButton::Left, move |_, window, cx| {
            let display_id = window.display(cx).map(|d| d.id());
            state_for_settings.borrow_mut().view_entity = None;
            window.remove_window();
            schedule_open_settings_window(state_for_settings.clone(), display_id, cx);
        });

        // 关闭按钮（圆形，红色调）
        let close_btn = self.render_circle_button(
            "src/icons/close.svg",
            cx.global::<Theme>().status_error,
            cx.global::<Theme>().btn_danger_bg,
            cx.global::<Theme>().btn_danger_bg,
        );
        let close_btn = close_btn.on_mouse_down(MouseButton::Left, |_, _, cx| {
            cx.quit();
        });

        div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(14.0))
            .py(px(10.0))
            .border_t_1()
            .border_color(border_color)
            .child(sync_btn)
            // 弹性空白，将设置和关闭按钮推到右侧
            .child(div().flex_1())
            .child(settings_btn)
            .child(close_btn)
    }

    /// 圆形工具栏按钮
    fn render_circle_button(
        &self,
        icon: &'static str,
        icon_color: Hsla,
        bg_color: Hsla,
        border_color: Hsla,
    ) -> Div {
        div()
            .w(px(38.0))
            .h(px(38.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(10.0))
            .bg(bg_color)
            .border_1()
            .border_color(border_color)
            .cursor_pointer()
            .child(widgets::render_svg_icon(icon, px(16.0), icon_color))
    }
}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.borrow();
        let active_tab = state.nav.active_tab;
        // 在每次渲染时动态调整窗口高度
        let desired_height = compute_popup_height(&state);
        drop(state);

        // 仅对 Windowed 类型窗口执行 resize（避免影响全屏/最大化窗口）
        let bounds = window.window_bounds();
        if let WindowBounds::Windowed(current_bounds) = bounds {
            let new_height = px(desired_height);
            let diff = current_bounds.size.height - new_height;
            if diff.abs() > px(2.0) {
                // 仅在高度变化时输出布局明细（避免每帧刷屏）
                let content_area = desired_height
                    - PopupLayout::HEADER_HEIGHT
                    - PopupLayout::NAV_HEIGHT
                    - PopupLayout::FOOTER_HEIGHT;
                debug!(
                    target: "layout",
                    "window resize: {:?} -> {:?} (diff={:?}), content_area={:.0}px",
                    current_bounds.size.height,
                    new_height,
                    diff,
                    content_area
                );
                window.resize(size(px(PopupLayout::WIDTH), new_height));
            }
        }

        let theme = cx.global::<Theme>();
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.bg_panel)
            .text_color(theme.text_primary)
            .rounded(px(14.0))
            .overflow_hidden()
            // 头部
            .child(self.render_header(cx))
            // 导航
            .child(self.render_top_nav(active_tab, cx))
            // 内容区
            .child(
                div()
                    .id("content")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(match active_tab {
                        NavTab::Provider(kind) => div()
                            .px(px(12.0))
                            .pt(px(10.0))
                            .pb(px(12.0))
                            .child(self.render_provider_detail(kind, cx))
                            .into_any_element(),
                        NavTab::Settings => self.render_settings_content(cx),
                    }),
            )
            // 底部工具栏
            .child(self.render_global_actions(active_tab, cx))
    }
}

impl Drop for AppView {
    fn drop(&mut self) {
        if let Ok(mut state) = self.state.try_borrow_mut() {
            state.view_entity = None;
        }
    }
}
