mod nav;
mod provider_logic;
mod provider_panel;
mod settings_window;
mod tray_settings;
mod widgets;

pub use settings_window::schedule_open_settings_window;
use settings_window::SettingsTab;

use crate::models::{AppSettings, AppTheme, ConnectionStatus, NavTab, ProviderKind};
use crate::theme::Theme;
use gpui::*;
use log::{info, warn};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

// ============================================================================
// 子状态结构 (SRP: 每个结构体负责一个独立职责)
// ============================================================================

/// Provider 数据与刷新调度
pub struct ProviderStore {
    pub providers: Vec<crate::models::ProviderStatus>,
    pub manager: Arc<crate::providers::ProviderManager>,
    /// When the last refresh cycle was started (None = never).
    /// Replaces a boolean flag so we can debounce and detect stale loops.
    pub last_refresh_started: Option<std::time::Instant>,
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
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new() -> Self {
        let settings = match crate::settings_store::load() {
            Ok(settings) => {
                info!(target: "settings", "loaded settings from {}", crate::settings_store::config_path().display());
                settings
            }
            Err(err) => {
                warn!(target: "settings", "failed to load saved settings: {err}");
                AppSettings::default()
            }
        };
        let manager = Arc::new(crate::providers::ProviderManager::new());
        let mut providers = manager.initial_statuses();
        // 从配置中恢复各 Provider 的启用状态
        for p in &mut providers {
            p.enabled = settings.is_provider_enabled(p.kind);
        }
        // 默认选第一个已启用的 Provider。如果都没有启用，直接切到设置 Tab
        let first_enabled = ProviderKind::all()
            .iter()
            .find(|k| settings.is_provider_enabled(**k))
            .copied();

        let active_tab = if let Some(kind) = first_enabled {
            NavTab::Provider(kind)
        } else {
            NavTab::Settings
        };

        Self {
            provider_store: ProviderStore {
                providers,
                manager,
                last_refresh_started: None,
            },
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
        }
    }

    /// Toggle a provider on/off and update all related state.
    /// Returns `(updated_settings, should_refresh)`.
    pub fn toggle_provider(&mut self, kind: ProviderKind) -> (AppSettings, bool) {
        let new_val = !self.settings.is_provider_enabled(kind);
        self.settings.set_provider_enabled(kind, new_val);

        if let Some(p) = self.provider_store.find_mut(kind) {
            p.enabled = new_val;
            if new_val {
                p.connection = ConnectionStatus::Refreshing;
            }
        }

        if new_val {
            self.provider_store.last_refresh_started = None;
            self.nav.switch_to(NavTab::Provider(kind));
        } else {
            self.nav.fallback_on_disable(kind, &self.settings);
        }

        (self.settings.clone(), new_val)
    }

    /// Refresh all enabled providers at startup (before any window is opened).
    pub fn spawn_startup_refresh(state: Rc<RefCell<Self>>, cx: &App) {
        let kinds: Vec<_> = {
            let s = state.borrow();
            ProviderKind::all()
                .iter()
                .filter(|k| s.settings.is_provider_enabled(**k))
                .copied()
                .collect()
        };

        if kinds.is_empty() {
            return;
        }

        info!(target: "providers", "startup refresh for {} enabled providers", kinds.len());
        {
            let mut s = state.borrow_mut();
            for kind in &kinds {
                s.provider_store
                    .set_connection(*kind, ConnectionStatus::Refreshing);
            }
            s.provider_store.last_refresh_started = Some(std::time::Instant::now());
        }

        for kind in kinds {
            Self::spawn_provider_refresh(state.clone(), kind, cx);
        }
    }

    /// Spawn an async task to refresh a single provider's data.
    /// Works from any window context (SettingsView, AppView, etc).
    pub fn spawn_provider_refresh(state: Rc<RefCell<Self>>, kind: ProviderKind, cx: &App) {
        let async_cx = cx.to_async();
        async_cx
            .foreground_executor()
            .spawn(async move {
                let manager = state.borrow().provider_store.manager.clone();
                let mgr = manager.clone();

                let available =
                    smol::unblock(move || smol::block_on(mgr.is_provider_available(kind))).await;

                if !available {
                    let mut s = state.borrow_mut();
                    if let Some(p) = s.provider_store.find_mut(kind) {
                        p.connection = ConnectionStatus::Disconnected;
                        p.error_message = Some("Provider is currently unavailable.".to_string());
                    }
                    return;
                }

                let result =
                    smol::unblock(move || smol::block_on(manager.refresh_provider(kind))).await;

                let mut s = state.borrow_mut();
                if let Some(p) = s.provider_store.find_mut(kind) {
                    match result {
                        Ok(quotas) => {
                            p.quotas = quotas;
                            p.connection = ConnectionStatus::Connected;
                            p.last_refreshed_instant = Some(std::time::Instant::now());
                            p.last_updated_at = None;
                            p.error_message = None;
                        }
                        Err(err) => {
                            p.connection = ConnectionStatus::Error;
                            p.last_updated_at = Some("Update failed".to_string());
                            p.error_message = Some(err.to_string());
                        }
                    }
                }
            })
            .detach();
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
    match crate::settings_store::save(settings) {
        Ok(path) => {
            info!(target: "settings", "saved settings to {}", path.display());
        }
        Err(err) => {
            warn!(target: "settings", "failed to save settings: {err}");
        }
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

        // 后台 refresh 循环绑定在 AppView 生命周期上，窗口关闭后循环会停。
        // 每次新建窗口需要重启循环，但不应立即重新拉取——
        // do_refresh_all 内部会用 per-provider 防抖跳过还在有效期内的 Provider。
        let should_restart_loop = state
            .borrow()
            .provider_store
            .last_refresh_started
            .map(|t| t.elapsed() > Duration::from_secs(5))
            .unwrap_or(true);

        if should_restart_loop {
            info!(target: "providers", "starting background refresh loop");
            state.borrow_mut().provider_store.last_refresh_started =
                Some(std::time::Instant::now());
            Self::start_background_refresh(state.borrow().provider_store.manager.clone(), cx);
        }

        Self {
            state,
            _activation_sub: None,
        }
    }

    /// 执行一轮完整的 provider 刷新
    async fn do_refresh_all(
        manager: &Arc<crate::providers::ProviderManager>,
        view: &gpui::WeakEntity<AppView>,
        async_cx: &mut gpui::AsyncApp,
    ) {
        let all_kinds = ProviderKind::all().to_vec();
        for kind in all_kinds {
            // 跳过未启用的 Provider
            let enabled = view
                .update(async_cx, |view, _| {
                    view.state.borrow().settings.is_provider_enabled(kind)
                })
                .unwrap_or(false);
            if !enabled {
                continue;
            }

            // 跳过最近刚刷新过的 Provider（防抖）
            let recently_refreshed = view
                .update(async_cx, |view, _| {
                    let s = view.state.borrow();
                    Self::is_recently_refreshed(&s.provider_store, &s.settings, kind)
                })
                .unwrap_or(false);
            if recently_refreshed {
                info!(target: "providers", "skipping provider {:?} (refreshed within cooldown)", kind);
                continue;
            }

            let mgr = manager.clone();

            // Check availability first (runs on background thread)
            let mgr_check = mgr.clone();
            let available =
                smol::unblock(move || smol::block_on(mgr_check.is_provider_available(kind))).await;

            if !available {
                info!(target: "providers", "skipping provider {:?} (unavailable)", kind);
                let _ = view.update(async_cx, |view, cx| {
                    let mut s = view.state.borrow_mut();
                    if let Some(p) = s.provider_store.find_mut(kind) {
                        if p.connection != ConnectionStatus::Connected {
                            p.connection = ConnectionStatus::Disconnected;
                        }
                    }
                    cx.notify();
                });
                continue;
            }

            // Set Refreshing state before starting
            let _ = view.update(async_cx, |view, cx| {
                view.state
                    .borrow_mut()
                    .provider_store
                    .set_connection(kind, ConnectionStatus::Refreshing);
                cx.notify();
            });

            info!(target: "providers", "refreshing provider {:?}", kind);
            let result = smol::unblock(move || smol::block_on(mgr.refresh_provider(kind))).await;

            match result {
                Ok(quotas) => {
                    info!(target: "providers", "provider {:?} refresh succeeded with {} quotas", kind, quotas.len());
                    let _ = view.update(async_cx, |view, cx| {
                        let mut s = view.state.borrow_mut();
                        if let Some(p) = s.provider_store.find_mut(kind) {
                            p.quotas = quotas;
                            p.connection = ConnectionStatus::Connected;
                            p.last_refreshed_instant = Some(std::time::Instant::now());
                            p.last_updated_at = None;
                            p.error_message = None;
                        }
                        cx.notify();
                    });
                }
                Err(err) => {
                    warn!(target: "providers", "provider {:?} refresh failed: {err}", kind);
                    let _ = view.update(async_cx, |view, cx| {
                        let mut s = view.state.borrow_mut();
                        if let Some(p) = s.provider_store.find_mut(kind) {
                            if p.quotas.is_empty() {
                                p.connection = ConnectionStatus::Error;
                            } else {
                                // Keep Connected if we have stale data
                                p.connection = ConnectionStatus::Connected;
                            }
                            p.last_updated_at = Some("Update failed".to_string());
                            p.error_message = Some(err.to_string());
                        }
                        cx.notify();
                    });
                }
            }
        }
    }

    fn start_background_refresh(
        manager: Arc<crate::providers::ProviderManager>,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(move |this: gpui::WeakEntity<AppView>, cx: &mut gpui::AsyncApp| {
            let mut async_cx = cx.clone();
            async move {
                // 首次刷新
                Self::do_refresh_all(&manager, &this, &mut async_cx).await;

                // 定时刷新：每次循环动态读取最新间隔
                loop {
                    let interval_mins = this
                        .update(&mut async_cx, |view, _| {
                            view.state.borrow().settings.refresh_interval_mins
                        })
                        .unwrap_or(0);

                    // 0 表示禁用自动刷新，短暂休眠后重新检查（设置可能被改回来）
                    if interval_mins == 0 {
                        smol::Timer::after(Duration::from_secs(5)).await;
                        if this.upgrade().is_none() {
                            break;
                        }
                        continue;
                    }

                    let interval = Duration::from_secs(interval_mins * 60);
                    smol::Timer::after(interval).await;

                    if this.upgrade().is_none() {
                        info!(target: "providers", "view dropped, stopping periodic refresh");
                        break;
                    }
                    info!(target: "providers", "periodic refresh triggered (every {} min)", interval_mins);
                    Self::do_refresh_all(&manager, &this, &mut async_cx).await;
                }
            }
        })
        .detach();
    }

    /// 获取 per-provider 防抖冷却时间：取刷新间隔的一半，但不低于 30 秒
    fn refresh_cooldown(settings: &AppSettings) -> Duration {
        let interval_secs = settings.refresh_interval_mins * 60;
        let half = interval_secs / 2;
        Duration::from_secs(half.max(30))
    }

    /// 检查某个 Provider 是否在冷却期内（最近刚刷新过）
    fn is_recently_refreshed(
        store: &ProviderStore,
        settings: &AppSettings,
        kind: ProviderKind,
    ) -> bool {
        if let Some(p) = store.find(kind) {
            if let Some(instant) = p.last_refreshed_instant {
                return instant.elapsed() < Self::refresh_cooldown(settings);
            }
        }
        false
    }

    fn render_global_actions(
        &self,
        active_tab: NavTab,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.global::<Theme>();
        let state = self.state.clone();

        let mut menu = div()
            .flex_col()
            .w_full()
            .px(px(8.0))
            .py(px(6.0))
            .bg(theme.bg_subtle)
            .border_t_1()
            .border_color(theme.border_subtle);

        if let NavTab::Provider(kind) = active_tab {
            let dashboard_url = state
                .borrow()
                .provider_store
                .find(kind)
                .map(|p| p.dashboard_url().to_string())
                .unwrap_or_default();
            menu = menu
                .child(self.render_menu_item(
                    "Status Page",
                    Some("src/icons/usage.svg"),
                    theme,
                    move |_, _, _| {
                        let cmd = if cfg!(target_os = "linux") {
                            "xdg-open"
                        } else {
                            "open"
                        };
                        let _ = std::process::Command::new(cmd).arg(&dashboard_url).spawn();
                    },
                ))
                .child(div().h(px(1.0)).bg(theme.border_subtle).my(px(3.0)));
        }

        let settings_state = state.clone();
        menu.child(
            self.render_menu_item("Settings...", None, theme, move |_, window, cx| {
                let display_id = window.display(cx).map(|d| d.id());
                window.remove_window();
                schedule_open_settings_window(settings_state.clone(), display_id, cx);
            }),
        )
        .child(self.render_menu_item("Quit", None, theme, |_, _, cx| {
            cx.quit();
        }))
    }

    fn render_menu_item<F>(
        &self,
        label: &'static str,
        icon_path: Option<&'static str>,
        theme: &Theme,
        on_click: F,
    ) -> impl IntoElement
    where
        F: Fn(&gpui::MouseDownEvent, &mut Window, &mut App) + 'static,
    {
        let item = div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .px(px(6.0))
            .py(px(4.0))
            .rounded(px(6.0))
            .cursor_pointer()
            .hover(|style| style.bg(theme.border_subtle));

        let item = if let Some(path) = icon_path {
            item.child(crate::app::widgets::render_svg_icon(
                path,
                px(14.0),
                theme.text_secondary,
            ))
        } else {
            item
        };

        item.child(
            div()
                .text_size(px(13.0))
                .text_color(theme.text_primary)
                .child(label),
        )
        .on_mouse_down(MouseButton::Left, on_click)
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
