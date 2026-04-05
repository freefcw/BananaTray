#![recursion_limit = "512"]

rust_i18n::i18n!("locales", fallback = "en");

mod app;
mod app_state;
mod application;
mod assets;
mod auto_launch;
mod i18n;
mod logging;
pub mod models;
pub mod notification;
mod provider_error_presenter;
mod providers;
mod refresh;
mod runtime;
mod settings_store;
mod single_instance;
mod theme;
mod tray_icon_helper;
mod utils;

use app::{schedule_open_settings_window, AppState};
use application::AppAction;
use assets::Assets;
use gpui::*;
use log::{error, info};
use models::NavTab;
use refresh::{RefreshCoordinator, RefreshReason, RefreshRequest};
use rust_i18n::t;
use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;

/// 窗口管理器：持有全局窗口句柄，纯数据，不含任何锁操作
struct TrayController {
    window: Option<WindowHandle<app::AppView>>,
    state: Rc<RefCell<AppState>>,
}

impl TrayController {
    fn new(
        refresh_tx: smol::channel::Sender<RefreshRequest>,
        manager: &crate::providers::ProviderManager,
        log_path: Option<std::path::PathBuf>,
    ) -> Self {
        info!(target: "tray", "initializing tray controller");
        let state = Rc::new(RefCell::new(AppState::new(refresh_tx, manager, log_path)));
        Self {
            window: None,
            state,
        }
    }

    /// Close the tray popup window and clear the view entity reference.
    /// Returns the display ID the popup was on, if available.
    fn close_popup(&mut self, cx: &mut App) -> Option<DisplayId> {
        let window = self.window.take()?;
        self.state.borrow_mut().view_entity = None;
        let mut display_id = None;
        let _ = window.update(cx, |_, window, cx| {
            display_id = window.display(cx).map(|d| d.id());
            window.remove_window();
        });
        display_id
    }

    /// Check if the window handle is actually valid (window still exists).
    fn is_window_alive(&self, cx: &mut App) -> bool {
        if let Some(handle) = self.window.as_ref() {
            // Try to update the window - if this fails, the handle is stale
            handle.update(cx, |_, _, _| {}).is_ok()
        } else {
            false
        }
    }

    fn toggle_provider(&mut self, cx: &mut App) {
        let provider_tab = {
            let mut state = self.state.borrow_mut();
            state.session.default_provider_tab()
        };

        let Some(provider_tab) = provider_tab else {
            info!(target: "tray", "no providers enabled, opening settings directly");
            self.show_settings(cx);
            return;
        };
        info!(target: "tray", "toggle provider panel for {:?}", provider_tab);

        // Check if window is actually alive, not just if handle exists
        if self.is_window_alive(cx) {
            let active_tab = self.state.borrow().session.nav.active_tab;
            if matches!(active_tab, NavTab::Provider(_)) {
                info!(target: "tray", "provider panel already open, closing existing panel");
                self.close_popup(cx);
            } else {
                info!(target: "tray", "reusing existing window handle for provider panel");
                self.show(provider_tab, cx);
            }
        } else {
            // Handle is stale, clear it
            info!(target: "tray", "window handle is stale, clearing and opening fresh panel");
            self.window = None;
            self.show(provider_tab, cx);
        }
    }

    fn show_settings(&mut self, cx: &mut App) {
        info!(target: "tray", "requested settings window from tray controller");
        let display_id = self.close_popup(cx);
        schedule_open_settings_window(self.state.clone(), display_id, cx);
    }

    fn show(&mut self, tab: NavTab, cx: &mut App) {
        info!(target: "tray", "show window for tab {:?}", tab);
        runtime::dispatch_in_app(&self.state, AppAction::SelectNavTab(tab), cx);

        if self.window.is_some() {
            info!(target: "tray", "reusing existing tray window");
        } else {
            info!(target: "tray", "opening a fresh tray window");
            self.open(cx);
        }
    }

    fn preferred_window_kind() -> WindowKind {
        if cfg!(target_os = "linux") {
            WindowKind::Floating
        } else {
            WindowKind::PopUp
        }
    }

    fn preferred_window_bounds(cx: &App, window_size: Size<Pixels>) -> Bounds<Pixels> {
        let tray_bounds = cx
            .tray_icon_bounds()
            .filter(|b| b.size.width > px(0.0) && b.size.height > px(0.0));

        let position = if let Some(tray_bounds) = tray_bounds {
            WindowPosition::TrayCenter(tray_bounds)
        } else if cfg!(target_os = "linux") {
            WindowPosition::TopRight { margin: px(16.0) }
        } else {
            WindowPosition::Center
        };

        cx.compute_window_bounds(window_size, &position)
    }

    fn open(&mut self, cx: &mut App) {
        let dynamic_height = self.state.borrow().session.popup_height();
        info!(target: "tray", "opening window with dynamic height: {}px", dynamic_height);
        let window_size = size(px(models::PopupLayout::WIDTH), px(dynamic_height));
        let bounds = Self::preferred_window_bounds(cx, window_size);
        let kind = Self::preferred_window_kind();

        let state = self.state.clone();

        let result = cx.open_window(
            WindowOptions {
                titlebar: None,
                kind,
                focus: true,
                show: true,
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_window, cx| cx.new(|cx| app::AppView::new(state, cx)),
        );

        if let Ok(handle) = result {
            info!(target: "tray", "tray popup opened successfully");
            // 监听窗口失焦，自动关闭
            let auto_hide_state = self.state.clone();
            let activation_initialized = Rc::new(Cell::new(false));
            let _ = handle.update(cx, |view, window, cx| {
                let activation_initialized = activation_initialized.clone();
                let sub = cx.observe_window_activation(window, move |_view, window, _cx| {
                    if !activation_initialized.replace(true) {
                        return;
                    }
                    let should_auto_hide =
                        auto_hide_state.borrow().session.settings.auto_hide_window;
                    if should_auto_hide && !window.is_window_active() {
                        info!(target: "tray", "auto-hide closing inactive tray popup");
                        auto_hide_state.borrow_mut().view_entity = None;
                        window.remove_window();
                    }
                });
                view._activation_sub = Some(sub);
            });
            self.window = Some(handle);
        } else if let Err(err) = result {
            error!(target: "tray", "failed to open tray popup: {err:?}");
        }
    }
}

fn main() {
    let log_path = match logging::init() {
        Ok(init) => {
            log::info!(target: "app", "logging initialized at {}", init.log_path.display());
            Some(init.log_path)
        }
        Err(err) => {
            eprintln!("failed to initialize logging: {err:#}");
            None
        }
    };

    // Single-instance check: must run before Application::new() so that a
    // secondary process exits immediately without initializing the UI toolkit.
    let show_rx = match single_instance::ensure_single_instance() {
        single_instance::InstanceRole::Primary(rx) => rx,
        single_instance::InstanceRole::Secondary => {
            info!(target: "app", "another instance is already running, exiting");
            std::process::exit(0);
        }
    };

    Application::new()
        .with_assets(Assets::new())
        .run(move |cx: &mut App| {
            // 0. 初始化 i18n locale
            {
                let settings = crate::settings_store::load().unwrap_or_default();
                crate::i18n::apply_locale(&settings.language);
            }

            // 1. 初始化
            adabraka_ui::init(cx);
            adabraka_ui::theme::install_theme(cx, adabraka_ui::theme::Theme::light());
            cx.set_keep_alive_without_windows(true);

            // 2. 配置系统托盘
            {
                let settings = crate::settings_store::load().unwrap_or_default();
                crate::tray_icon_helper::apply_tray_icon(cx, settings.tray_icon_style);
            }
            cx.set_tray_tooltip(&t!("tray.tooltip"));
            cx.set_tray_panel_mode(true);

            // 3. 启动 RefreshCoordinator（后台事件循环）
            let (event_tx, event_rx) = smol::channel::bounded::<refresh::RefreshEvent>(64);
            let manager = std::sync::Arc::new(crate::providers::ProviderManager::new());
            let coordinator = RefreshCoordinator::new(manager.clone(), event_tx);
            let refresh_tx = coordinator.sender();

            // 在后台线程运行协调器事件循环
            std::thread::Builder::new()
                .name("refresh-coordinator".into())
                .spawn(move || smol::block_on(coordinator.run()))
                .expect("failed to spawn refresh coordinator thread");

            // 4. 窗口控制器（复用同一个 ProviderManager 实例）
            let controller = Rc::new(RefCell::new(TrayController::new(
                refresh_tx,
                &manager,
                log_path.clone(),
            )));

            // 5. 启动事件泵：从协调器接收事件，更新 AppState，并通知 UI 刷新
            {
                let state = controller.borrow().state.clone();
                let async_cx = cx.to_async();
                let pump_cx = cx.to_async();
                async_cx
                    .foreground_executor()
                    .spawn(async move {
                        while let Ok(event) = event_rx.recv().await {
                            let _ = pump_cx.update(|cx| {
                                runtime::dispatch_in_app(
                                    &state,
                                    AppAction::RefreshEventReceived(event),
                                    cx,
                                );
                            });
                        }
                    })
                    .detach();
            }

            // 6. 初始配置同步 + 启动刷新
            {
                let state = controller.borrow().state.clone();
                let config_request =
                    crate::application::build_config_sync_request(&state.borrow().session);
                let _ = state.borrow().send_refresh(config_request);
                let _ = state.borrow().send_refresh(RefreshRequest::RefreshAll {
                    reason: RefreshReason::Startup,
                });
            }

            // 7. 托盘点击
            let tray_ctrl = controller.clone();
            cx.on_tray_icon_event(move |event, cx| {
                info!(target: "tray", "received tray event: {:?}", event);
                match event {
                    TrayIconEvent::LeftClick => tray_ctrl.borrow_mut().toggle_provider(cx),
                    TrayIconEvent::RightClick => tray_ctrl.borrow_mut().show_settings(cx),
                    _ => {}
                }
            });

            // 8. 全局热键 Cmd+Shift+S
            info!(target: "hotkey", "registering global hotkey Cmd+Shift+S");
            if let Ok(keystroke) = Keystroke::parse("cmd-shift-s") {
                let _ = cx.register_global_hotkey(1, &keystroke);
            }
            let async_cx = cx.to_async();
            let hotkey_ctrl = controller.clone();
            cx.on_global_hotkey(move |id| {
                if id == 1 {
                    info!(target: "hotkey", "received global hotkey 1");
                    let _ = async_cx.update(|cx| {
                        hotkey_ctrl.borrow_mut().toggle_provider(cx);
                    });
                }
            });

            // 9. Listen for "SHOW" commands from secondary instances.
            //    Bridge std::sync::mpsc → smol::channel so we can await on the
            //    foreground executor (std Receiver is !Sync).
            {
                let (show_async_tx, show_async_rx) = smol::channel::bounded::<()>(4);
                std::thread::Builder::new()
                    .name("single-instance-bridge".into())
                    .spawn(move || {
                        while show_rx.recv().is_ok() {
                            if show_async_tx.send_blocking(()).is_err() {
                                break;
                            }
                        }
                    })
                    .expect("failed to spawn single-instance bridge thread");

                let show_ctrl = controller.clone();
                let show_async_cx = cx.to_async();
                cx.to_async()
                    .foreground_executor()
                    .spawn(async move {
                        while show_async_rx.recv().await.is_ok() {
                            info!(target: "app", "secondary instance requested SHOW");
                            let _ = show_async_cx.update(|cx| {
                                show_ctrl.borrow_mut().toggle_provider(cx);
                            });
                        }
                    })
                    .detach();
            }

            info!(target: "app", "BananaTray is running - look for the tray icon");
        });
}
