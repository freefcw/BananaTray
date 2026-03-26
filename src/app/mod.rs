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
// 外部持久状态 (不随窗口销毁)
// ============================================================================

/// 应用持久状态，在窗口生命周期之外保持
pub struct AppState {
    pub providers: Vec<crate::models::ProviderStatus>,
    pub settings: AppSettings,
    pub active_tab: NavTab,
    pub last_provider_kind: ProviderKind,
    pub manager: Arc<crate::providers::ProviderManager>,
    pub refreshed: bool,
    pub settings_tab: SettingsTab,
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
        let providers = manager.initial_statuses();
        Self {
            providers,
            settings,
            active_tab: NavTab::Provider(ProviderKind::Claude),
            last_provider_kind: ProviderKind::Claude,
            manager,
            refreshed: false,
            settings_tab: SettingsTab::General,
        }
    }
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

        // 只在首次打开时刷新 provider 数据
        if !state.borrow().refreshed {
            info!(target: "providers", "starting first background refresh pass");
            state.borrow_mut().refreshed = true;
            let refresh_mins = state.borrow().settings.refresh_interval_mins;
            Self::start_background_refresh(state.borrow().manager.clone(), refresh_mins, cx);
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
            let mgr = manager.clone();

            // Check availability first (runs on background thread)
            let mgr_check = mgr.clone();
            let available =
                smol::unblock(move || smol::block_on(mgr_check.is_provider_available(kind))).await;

            if !available {
                info!(target: "providers", "skipping provider {:?} (unavailable)", kind);
                let _ = view.update(async_cx, |view, cx| {
                    let mut s = view.state.borrow_mut();
                    if let Some(p) = s.providers.iter_mut().find(|p| p.kind == kind) {
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
                let mut s = view.state.borrow_mut();
                if let Some(p) = s.providers.iter_mut().find(|p| p.kind == kind) {
                    p.connection = ConnectionStatus::Refreshing;
                }
                cx.notify();
            });

            info!(target: "providers", "refreshing provider {:?}", kind);
            let result = smol::unblock(move || smol::block_on(mgr.refresh_provider(kind))).await;

            match result {
                Ok(quotas) => {
                    info!(target: "providers", "provider {:?} refresh succeeded with {} quotas", kind, quotas.len());
                    let _ = view.update(async_cx, |view, cx| {
                        let mut s = view.state.borrow_mut();
                        if let Some(p) = s.providers.iter_mut().find(|p| p.kind == kind) {
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
                        if let Some(p) = s.providers.iter_mut().find(|p| p.kind == kind) {
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
        refresh_interval_mins: u64,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(move |this: gpui::WeakEntity<AppView>, cx: &mut gpui::AsyncApp| {
            let mut async_cx = cx.clone();
            async move {
                // 首次刷新
                Self::do_refresh_all(&manager, &this, &mut async_cx).await;

                // 定时刷新（0 表示禁用）
                if refresh_interval_mins > 0 {
                    let interval = Duration::from_secs(refresh_interval_mins * 60);
                    loop {
                        smol::Timer::after(interval).await;
                        // 如果 view 已销毁，退出循环
                        if this.upgrade().is_none() {
                            info!(target: "providers", "view dropped, stopping periodic refresh");
                            break;
                        }
                        info!(target: "providers", "periodic refresh triggered (every {} min)", refresh_interval_mins);
                        Self::do_refresh_all(&manager, &this, &mut async_cx).await;
                    }
                }
            }
        })
        .detach();
    }
}

impl Render for AppView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<Theme>();
        let state = self.state.borrow();
        let active_tab = state.active_tab;
        drop(state);

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
                            .px(px(12.0))
                            .py(px(10.0))
                            .child(self.render_provider_detail(kind, cx))
                            .into_any_element(),
                        NavTab::Settings => self.render_settings_content(cx),
                    }),
            )
    }
}
