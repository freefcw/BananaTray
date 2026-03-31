mod nav;
mod provider_logic;
mod provider_panel;
mod settings_window;
mod tray_settings;
mod widgets;

pub use settings_window::schedule_open_settings_window;

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
            },
            settings_ui: SettingsUiState {
                active_tab: SettingsTab::General,
                selected_provider: ProviderKind::Claude,
                cadence_dropdown_open: false,
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
                    RefreshResult::Failed { error } => {
                        p.mark_refresh_failed(error);
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
    let quota_count = state
        .provider_store
        .find(kind)
        .map(|p| p.quotas.len())
        .unwrap_or(1)
        .max(1);
    crate::models::compute_popup_height_for_quotas(quota_count)
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
}

impl AppView {
    pub fn new(state: Rc<RefCell<AppState>>, cx: &mut Context<Self>) -> Self {
        let theme = match state.borrow().settings.theme {
            AppTheme::Light => Theme::light(),
            AppTheme::Dark => Theme::dark(),
        };
        cx.set_global(theme);

        state.borrow_mut().view_entity = Some(cx.entity().downgrade());

        Self {
            state,
            _activation_sub: None,
        }
    }

    fn render_global_actions(
        &self,
        active_tab: NavTab,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.global::<Theme>();
        let border_color = theme.border_subtle;

        let mut left = div().flex().items_center().gap(px(6.0));
        if let NavTab::Provider(kind) = active_tab {
            let borrowed = self.state.borrow();
            let dashboard_url = borrowed
                .provider_store
                .find(kind)
                .map(|p| p.dashboard_url().to_string())
                .unwrap_or_default();
            let is_refreshing = borrowed
                .provider_store
                .find(kind)
                .map(|p| p.connection == crate::models::ConnectionStatus::Refreshing)
                .unwrap_or(false);
            let show_dashboard = borrowed.settings.show_toolbar_dashboard;
            let show_refresh = borrowed.settings.show_toolbar_refresh;
            drop(borrowed);

            if show_dashboard {
                left = left.child(widgets::with_tooltip(
                    "tt-dashboard",
                    &t!("tooltip.dashboard"),
                    theme,
                    Self::render_dashboard_button(dashboard_url, theme),
                ));
            }
            if show_refresh {
                let refresh_btn = self.render_refresh_button(kind, is_refreshing, cx);
                let theme = cx.global::<Theme>();
                left = left.child(widgets::with_tooltip(
                    "tt-refresh",
                    &t!("tooltip.refresh"),
                    theme,
                    refresh_btn,
                ));
            }
        }

        let right = {
            let settings_btn = self.render_settings_icon_button(cx);
            let theme = cx.global::<Theme>();
            let close_btn = Self::render_close_button(theme);
            div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .child(widgets::with_tooltip(
                    "tt-settings",
                    &t!("tooltip.settings"),
                    theme,
                    settings_btn,
                ))
                .child(widgets::with_tooltip(
                    "tt-quit",
                    &t!("tooltip.quit"),
                    theme,
                    close_btn,
                ))
        };

        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .px(px(8.0))
            .py(px(6.0))
            .border_t_1()
            .border_color(border_color)
            .child(left)
            .child(right)
    }

    fn render_dashboard_button(url: String, theme: &Theme) -> Div {
        div()
            .flex()
            .items_center()
            .justify_center()
            .w(px(30.0))
            .h(px(30.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(theme.border_subtle)
            .bg(theme.bg_panel)
            .cursor_pointer()
            .hover(|style| style.bg(theme.bg_subtle))
            .child(crate::app::widgets::render_svg_icon(
                "src/icons/compass.svg",
                px(15.0),
                theme.text_accent,
            ))
            .on_mouse_down(MouseButton::Left, move |_, _, _| {
                let cmd = if cfg!(target_os = "linux") {
                    "xdg-open"
                } else {
                    "open"
                };
                let _ = std::process::Command::new(cmd).arg(&url).spawn();
            })
    }

    fn render_refresh_button(
        &self,
        kind: ProviderKind,
        is_refreshing: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = cx.global::<Theme>();
        let entity = cx.entity().clone();

        let icon_color = theme.text_accent;

        let mut btn = div()
            .flex()
            .items_center()
            .justify_center()
            .w(px(30.0))
            .h(px(30.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(theme.border_subtle)
            .bg(theme.bg_panel)
            .child(crate::app::widgets::render_svg_icon(
                "src/icons/refresh.svg",
                px(15.0),
                icon_color,
            ));

        if !is_refreshing {
            btn = btn
                .cursor_pointer()
                .hover(|style| style.bg(theme.bg_subtle))
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    entity.update(cx, |view, cx| {
                        view.refresh_single_provider(kind, cx);
                    });
                });
        }

        btn
    }

    fn render_settings_icon_button(&self, cx: &mut Context<Self>) -> Div {
        let theme = cx.global::<Theme>();
        let state = self.state.clone();

        div()
            .w(px(30.0))
            .h(px(30.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(8.0))
            .border_1()
            .border_color(theme.border_subtle)
            .bg(theme.bg_panel)
            .cursor_pointer()
            .hover(|style| style.bg(theme.bg_subtle))
            .child(crate::app::widgets::render_svg_icon(
                "src/icons/settings.svg",
                px(15.0),
                theme.text_secondary,
            ))
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                let display_id = window.display(cx).map(|d| d.id());
                state.borrow_mut().view_entity = None;
                window.remove_window();
                schedule_open_settings_window(state.clone(), display_id, cx);
            })
    }

    fn render_close_button(theme: &Theme) -> Div {
        div()
            .w(px(30.0))
            .h(px(30.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(8.0))
            .border_1()
            .border_color(theme.border_subtle)
            .bg(theme.bg_panel)
            .cursor_pointer()
            .hover(|style| style.bg(theme.bg_subtle))
            .child(crate::app::widgets::render_svg_icon(
                "src/icons/close.svg",
                px(15.0),
                theme.text_secondary,
            ))
            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                cx.quit();
            })
    }
}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<Theme>();
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
                window.resize(size(px(PopupLayout::WIDTH), new_height));
            }
        }

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.bg_panel)
            .text_color(theme.text_primary)
            .child(self.render_top_nav(active_tab, cx))
            .child(
                div()
                    .id("content")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(match active_tab {
                        NavTab::Provider(kind) => div()
                            .px(px(8.0)) // 更小边距
                            .py(px(4.0)) // 更小边距
                            .child(self.render_provider_detail(kind, cx))
                            .into_any_element(),
                        NavTab::Settings => self.render_settings_content(cx),
                    }),
            )
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
