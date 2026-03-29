mod nav;
mod provider_logic;
mod provider_panel;
mod settings_window;
mod tray_settings;
mod widgets;

pub use settings_window::schedule_open_settings_window;
use settings_window::SettingsTab;

use crate::models::{AppSettings, AppTheme, ConnectionStatus, NavTab, ProviderKind};
use crate::refresh::{RefreshEvent, RefreshReason, RefreshRequest, RefreshResult};
use crate::theme::Theme;
use gpui::*;
use log::{debug, info, warn};
use smol::channel::Sender;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

// ============================================================================
// 子状态结构 (SRP: 每个结构体负责一个独立职责)
// ============================================================================

/// Provider 数据存储
pub struct ProviderStore {
    pub providers: Vec<crate::models::ProviderStatus>,
    pub manager: Arc<crate::providers::ProviderManager>,
}

impl ProviderStore {
    pub fn find(&self, kind: ProviderKind) -> Option<&crate::models::ProviderStatus> {
        self.providers.iter().find(|p| p.kind == kind)
    }

    pub fn find_mut(&mut self, kind: ProviderKind) -> Option<&mut crate::models::ProviderStatus> {
        self.providers.iter_mut().find(|p| p.kind == kind)
    }

    pub fn set_connection(&mut self, kind: ProviderKind, status: ConnectionStatus) {
        if let Some(p) = self.find_mut(kind) {
            p.connection = status;
        }
    }
}

/// Tray 弹出窗口的导航状态
pub struct NavigationState {
    pub active_tab: NavTab,
    pub last_provider_kind: ProviderKind,
}

impl NavigationState {
    /// 切换到指定 tab，若为 Provider 则同步 last_provider_kind
    pub fn switch_to(&mut self, tab: NavTab) {
        self.active_tab = tab;
        if let NavTab::Provider(kind) = tab {
            self.last_provider_kind = kind;
        }
    }

    /// 当某个 provider 被禁用时，若它是当前活跃 tab 则回退到下一个已启用的 provider
    pub fn fallback_on_disable(&mut self, disabled: ProviderKind, settings: &AppSettings) {
        let is_current = matches!(self.active_tab, NavTab::Provider(k) if k == disabled);
        if !is_current {
            return;
        }
        if let Some(next) = ProviderKind::all()
            .iter()
            .find(|k| **k != disabled && settings.is_provider_enabled(**k))
            .copied()
        {
            self.switch_to(NavTab::Provider(next));
        }
    }
}

/// 设置窗口的临时 UI 状态
pub struct SettingsUiState {
    pub active_tab: SettingsTab,
    pub selected_provider: ProviderKind,
    pub cadence_dropdown_open: bool,
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
            provider_store: ProviderStore { providers, manager },
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
            view_entity: None,
        }
    }

    /// 向 RefreshCoordinator 发送请求（非阻塞）
    pub fn send_refresh(&self, request: RefreshRequest) {
        if let Err(err) = self.refresh_tx.try_send(request) {
            warn!(target: "refresh", "failed to send refresh request: {}", err);
        }
    }

    /// 通知协调器配置已变更
    pub fn sync_config_to_coordinator(&self) {
        let enabled: Vec<ProviderKind> = ProviderKind::all()
            .iter()
            .filter(|k| self.settings.is_provider_enabled(**k))
            .copied()
            .collect();
        self.send_refresh(RefreshRequest::UpdateConfig {
            interval_mins: self.settings.refresh_interval_mins,
            enabled,
        });
    }

    /// 统一处理来自 RefreshCoordinator 的事件，更新 Provider 状态
    /// 这是 **唯一** 修改 provider 连接状态的入口
    pub fn apply_refresh_event(&mut self, event: RefreshEvent) {
        match event {
            RefreshEvent::Started { kind } => {
                self.provider_store
                    .set_connection(kind, ConnectionStatus::Refreshing);
            }
            RefreshEvent::Finished(outcome) => {
                let Some(p) = self.provider_store.find_mut(outcome.kind) else {
                    return;
                };
                match outcome.result {
                    RefreshResult::Success { quotas } => {
                        info!(target: "providers", "provider {:?} refresh succeeded: {} quotas", outcome.kind, quotas.len());
                        p.quotas = quotas;
                        p.connection = ConnectionStatus::Connected;
                        p.last_refreshed_instant = Some(std::time::Instant::now());
                        p.last_updated_at = None;
                        p.error_message = None;
                    }
                    RefreshResult::Unavailable { message } => {
                        debug!(target: "providers", "provider {:?} unavailable: {}", outcome.kind, message);
                        if p.connection != ConnectionStatus::Connected {
                            p.connection = ConnectionStatus::Disconnected;
                        }
                        p.error_message = Some(message);
                    }
                    RefreshResult::Failed { error } => {
                        warn!(target: "providers", "provider {:?} refresh failed: {}", outcome.kind, error);
                        if p.quotas.is_empty() {
                            p.connection = ConnectionStatus::Error;
                        } else {
                            p.connection = ConnectionStatus::Connected;
                        }
                        p.last_updated_at = Some("Update failed".to_string());
                        p.error_message = Some(error);
                    }
                    RefreshResult::SkippedCooldown
                    | RefreshResult::SkippedInFlight
                    | RefreshResult::SkippedDisabled => {}
                }
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
            if new_val {
                p.connection = ConnectionStatus::Refreshing;
            }
        }

        if new_val {
            self.nav.switch_to(NavTab::Provider(kind));
        } else {
            self.nav.fallback_on_disable(kind, &self.settings);
        }

        // 通知协调器配置变更，并请求刷新
        self.sync_config_to_coordinator();
        if new_val {
            self.send_refresh(RefreshRequest::RefreshOne {
                kind,
                reason: RefreshReason::ProviderToggled,
            });
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
        let state = self.state.clone();
        let entity = cx.entity().clone();

        // ── 左侧胶囊按钮组 ──
        let mut left = div().flex().items_center().gap(px(6.0));

        if let NavTab::Provider(kind) = active_tab {
            let borrowed = state.borrow();
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
            drop(borrowed);

            // Dashboard 胶囊按钮
            left = left.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(5.0))
                    .px(px(10.0))
                    .py(px(5.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(theme.border_subtle)
                    .bg(theme.bg_panel)
                    .cursor_pointer()
                    .hover(|style| style.bg(theme.bg_subtle))
                    .child(crate::app::widgets::render_svg_icon(
                        "src/icons/usage.svg",
                        px(13.0),
                        theme.text_accent,
                    ))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text_primary)
                            .child("Dashboard"),
                    )
                    .on_mouse_down(MouseButton::Left, move |_, _, _| {
                        let cmd = if cfg!(target_os = "linux") {
                            "xdg-open"
                        } else {
                            "open"
                        };
                        let _ = std::process::Command::new(cmd).arg(&dashboard_url).spawn();
                    }),
            );

            // Refresh / Syncing 胶囊按钮
            let refresh_entity = entity.clone();
            let (refresh_label, refresh_icon_color, refresh_text_color) = if is_refreshing {
                ("Syncing", theme.text_accent, theme.text_accent)
            } else {
                ("Refresh", theme.text_accent, theme.text_primary)
            };
            let mut refresh_btn = div()
                .flex()
                .items_center()
                .gap(px(5.0))
                .px(px(10.0))
                .py(px(5.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(theme.border_subtle)
                .bg(theme.bg_panel)
                .child(crate::app::widgets::render_svg_icon(
                    "src/icons/refresh.svg",
                    px(13.0),
                    refresh_icon_color,
                ))
                .child(
                    div()
                        .text_size(px(12.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(refresh_text_color)
                        .child(refresh_label),
                );
            if !is_refreshing {
                refresh_btn = refresh_btn
                    .cursor_pointer()
                    .hover(|style| style.bg(theme.bg_subtle))
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        refresh_entity.update(cx, |view, cx| {
                            view.refresh_single_provider(kind, cx);
                        });
                    });
            }
            left = left.child(refresh_btn);
        }

        // ── 右侧图标按钮组 ──
        let settings_state = state.clone();
        let right = div()
            .flex()
            .items_center()
            .gap(px(4.0))
            // Settings 齿轮按钮
            .child(
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
                        window.remove_window();
                        schedule_open_settings_window(settings_state.clone(), display_id, cx);
                    }),
            )
            // Close X 按钮
            .child(
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
                    }),
            );

        // ── 组合底栏 ──
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .px(px(8.0))
            .py(px(6.0))
            .border_t_1()
            .border_color(theme.border_subtle)
            .child(left)
            .child(right)
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
